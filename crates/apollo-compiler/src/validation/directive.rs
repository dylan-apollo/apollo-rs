use super::CycleError;
use crate::ast;
use crate::collections::HashMap;
use crate::collections::HashSet;
use crate::coordinate::DirectiveArgumentCoordinate;
use crate::coordinate::DirectiveCoordinate;
use crate::schema;
use crate::schema::validation::BuiltInScalars;
use crate::validation::diagnostics::DiagnosticData;
use crate::validation::DiagnosticList;
use crate::validation::RecursionGuard;
use crate::validation::RecursionStack;
use crate::validation::SourceSpan;
use crate::Node;

/// This struct just groups functions that are used to find self-referential directives.
/// The way to use it is to call `FindRecursiveDirective::check`.
struct FindRecursiveDirective<'s> {
    schema: &'s schema::Schema,
}

impl FindRecursiveDirective<'_> {
    fn type_definition(
        &self,
        seen: &mut RecursionGuard<'_>,
        def: &schema::ExtendedType,
    ) -> Result<(), CycleError<ast::Directive>> {
        match def {
            schema::ExtendedType::Scalar(scalar_type_definition) => {
                self.directives(seen, &scalar_type_definition.directives)?;
            }
            schema::ExtendedType::Object(object_type_definition) => {
                self.directives(seen, &object_type_definition.directives)?;
            }
            schema::ExtendedType::Interface(interface_type_definition) => {
                self.directives(seen, &interface_type_definition.directives)?;
            }
            schema::ExtendedType::Union(union_type_definition) => {
                self.directives(seen, &union_type_definition.directives)?;
            }
            schema::ExtendedType::Enum(enum_type_definition) => {
                self.directives(seen, &enum_type_definition.directives)?;
                for enum_value in enum_type_definition.values.values() {
                    self.enum_value(seen, enum_value)?;
                }
            }
            schema::ExtendedType::InputObject(input_type_definition) => {
                self.directives(seen, &input_type_definition.directives)?;
                for input_value in input_type_definition.fields.values() {
                    self.input_value(seen, input_value)?;
                }
            }
        }

        Ok(())
    }

    fn input_value(
        &self,
        seen: &mut RecursionGuard<'_>,
        input_value: &Node<ast::InputValueDefinition>,
    ) -> Result<(), CycleError<ast::Directive>> {
        for directive in &input_value.directives {
            self.directive(seen, directive)?;
        }

        let type_name = input_value.ty.inner_named_type();
        if let Some(type_def) = self.schema.types.get(type_name) {
            self.type_definition(seen, type_def)?;
        }

        Ok(())
    }

    fn enum_value(
        &self,
        seen: &mut RecursionGuard<'_>,
        enum_value: &Node<ast::EnumValueDefinition>,
    ) -> Result<(), CycleError<ast::Directive>> {
        for directive in &enum_value.directives {
            self.directive(seen, directive)?;
        }

        Ok(())
    }

    fn directives(
        &self,
        seen: &mut RecursionGuard<'_>,
        directives: &[schema::Component<ast::Directive>],
    ) -> Result<(), CycleError<ast::Directive>> {
        for directive in directives {
            self.directive(seen, directive)?;
        }
        Ok(())
    }

    fn directive(
        &self,
        seen: &mut RecursionGuard<'_>,
        directive: &Node<ast::Directive>,
    ) -> Result<(), CycleError<ast::Directive>> {
        if !seen.contains(&directive.name) {
            if let Some(def) = self.schema.directive_definitions.get(&directive.name) {
                self.directive_definition(seen.push(&directive.name)?, def)
                    .map_err(|error| error.trace(directive))?;
            }
        } else if seen.first() == Some(&directive.name) {
            // Only report an error & bail out early if this is the *initial* directive.
            // This prevents raising confusing errors when a directive `@b` which is not
            // self-referential uses a directive `@a` that *is*. The error with `@a` should
            // only be reported on its definition, not on `@b`'s.
            return Err(CycleError::Recursed(vec![directive.clone()]));
        }

        Ok(())
    }

    fn directive_definition(
        &self,
        mut seen: RecursionGuard<'_>,
        def: &Node<ast::DirectiveDefinition>,
    ) -> Result<(), CycleError<ast::Directive>> {
        for input_value in &def.arguments {
            self.input_value(&mut seen, input_value)?;
        }

        Ok(())
    }

    fn check(
        schema: &schema::Schema,
        directive_def: &Node<ast::DirectiveDefinition>,
    ) -> Result<(), CycleError<ast::Directive>> {
        let mut recursion_stack = RecursionStack::with_root(directive_def.name.clone());
        FindRecursiveDirective { schema }
            .directive_definition(recursion_stack.guard(), directive_def)
    }
}

pub(crate) fn validate_directive_definition(
    diagnostics: &mut DiagnosticList,
    schema: &crate::Schema,
    built_in_scalars: &mut BuiltInScalars,
    def: &Node<ast::DirectiveDefinition>,
) {
    super::input_object::validate_argument_definitions(
        diagnostics,
        schema,
        built_in_scalars,
        &def.arguments,
        ast::DirectiveLocation::ArgumentDefinition,
    );

    let head_location = SourceSpan::recompose(def.location(), def.name.location());

    // A directive definition must not contain the use of a directive which
    // references itself directly.
    //
    // Returns Recursive Definition error.
    match FindRecursiveDirective::check(schema, def) {
        Ok(_) => {}
        Err(CycleError::Recursed(trace)) => {
            diagnostics.push(
                head_location,
                DiagnosticData::RecursiveDirectiveDefinition {
                    name: def.name.clone(),
                    trace,
                },
            );
        }
        Err(CycleError::Limit(_)) => diagnostics.push(
            head_location,
            DiagnosticData::DeeplyNestedType {
                name: def.name.clone(),
                describe_type: "directive",
            },
        ),
    }
}

pub(crate) fn validate_directive_definitions(
    diagnostics: &mut DiagnosticList,
    schema: &crate::Schema,
    built_in_scalars: &mut BuiltInScalars,
) {
    for directive_definition in schema.directive_definitions.values() {
        validate_directive_definition(diagnostics, schema, built_in_scalars, directive_definition);
    }
}

// TODO(@goto-bus-stop) This is a big function: should probably not be generic over the iterator
// type
pub(crate) fn validate_directives<'dir>(
    diagnostics: &mut DiagnosticList,
    schema: Option<&crate::Schema>,
    dirs: impl Iterator<Item = &'dir Node<ast::Directive>>,
    dir_loc: ast::DirectiveLocation,
    var_defs: &[Node<ast::VariableDefinition>],
) {
    let mut seen_directives = HashMap::<_, Option<SourceSpan>>::default();

    for dir in dirs {
        super::argument::validate_arguments(diagnostics, &dir.arguments);

        let name = &dir.name;
        let loc = dir.location();
        let directive_definition =
            schema.and_then(|schema| Some((schema, schema.directive_definitions.get(name)?)));

        if let Some(&original_loc) = seen_directives.get(name) {
            let is_repeatable = directive_definition
                .map(|(_, def)| def.repeatable)
                // Assume unknown directives are repeatable to avoid producing confusing diagnostics
                .unwrap_or(true);

            if !is_repeatable {
                diagnostics.push(
                    loc,
                    DiagnosticData::UniqueDirective {
                        name: name.clone(),
                        original_application: original_loc,
                    },
                );
            }
        } else {
            let loc = SourceSpan::recompose(dir.location(), dir.name.location());
            seen_directives.insert(&dir.name, loc);
        }

        if let Some((schema, directive_definition)) = directive_definition {
            let allowed_loc: HashSet<ast::DirectiveLocation> =
                HashSet::from_iter(directive_definition.locations.iter().cloned());
            if !allowed_loc.contains(&dir_loc) {
                diagnostics.push(
                    loc,
                    DiagnosticData::UnsupportedLocation {
                        name: name.clone(),
                        location: dir_loc,
                        valid_locations: directive_definition.locations.clone(),
                        definition_location: directive_definition.location(),
                    },
                );
            }

            for argument in &dir.arguments {
                let input_value = directive_definition
                    .arguments
                    .iter()
                    .find(|val| val.name == argument.name);

                // @b(a: true)
                if let Some(input_value) = input_value {
                    // TODO(@goto-bus-stop) do we really need value validation and variable
                    // validation separately?
                    if super::variable::validate_variable_usage(
                        diagnostics,
                        input_value,
                        var_defs,
                        argument,
                    )
                    .is_ok()
                    {
                        super::value::validate_values(
                            diagnostics,
                            schema,
                            &input_value.ty,
                            argument,
                            var_defs,
                        );
                    }
                } else {
                    diagnostics.push(
                        argument.location(),
                        DiagnosticData::UndefinedArgument {
                            name: argument.name.clone(),
                            coordinate: DirectiveCoordinate {
                                directive: dir.name.clone(),
                            }
                            .into(),
                            definition_location: loc,
                        },
                    );
                }
            }
            for arg_def in &directive_definition.arguments {
                let arg_value = dir
                    .arguments
                    .iter()
                    .find_map(|arg| (arg.name == arg_def.name).then_some(&arg.value));
                let is_null = match arg_value {
                    None => true,
                    // Prevents explicitly providing `requiredArg: null`,
                    // but you can still indirectly do the wrong thing by typing `requiredArg: $mayBeNull`
                    // and it won't raise a validation error at this stage.
                    Some(value) => value.is_null(),
                };

                if arg_def.is_required() && is_null {
                    diagnostics.push(
                        dir.location(),
                        DiagnosticData::RequiredArgument {
                            name: arg_def.name.clone(),
                            expected_type: arg_def.ty.clone(),
                            coordinate: DirectiveArgumentCoordinate {
                                directive: directive_definition.name.clone(),
                                argument: arg_def.name.clone(),
                            }
                            .into(),
                            definition_location: arg_def.location(),
                        },
                    );
                }
            }
        } else {
            diagnostics.push(
                loc,
                DiagnosticData::UndefinedDirective { name: name.clone() },
            )
        }
    }
}
