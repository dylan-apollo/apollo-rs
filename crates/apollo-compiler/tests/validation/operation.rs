use apollo_compiler::ApolloCompiler;
use apollo_compiler::ReprDatabase;

#[test]
fn it_fails_validation_with_missing_ident() {
    let input = r#"
query {
  cat {
    name
  }
}

query getPet {
  cat {
    name
  }
}

query getOtherPet {
  cat {
    nickname
  }
}

type Query {
  cat: Cat
}

type Cat {
  name: String
  nickname: String
  meowVolume: Int
}
"#;
    let mut compiler = ApolloCompiler::new();
    let id = compiler.add_document(input, "doc.graphql");

    let schema = compiler.db.schema();
    let diagnostics = compiler
        .db
        .executable_document(id)
        .validate(&schema)
        .unwrap_err();
    let diagnostics = format!("{diagnostics:#}");
    assert!(
        diagnostics.contains(
            "anonymous operation cannot be selected when the document contains other operations"
        ),
        "{diagnostics}"
    );
}

#[test]
fn it_fails_validation_with_duplicate_operation_names() {
    let input = r#"
query getName {
  cat {
    name
  }
}

query getName {
  cat {
    name
    nickname
  }
}

type Query {
  cat: Pet
}

union CatOrDog = Cat | Dog

interface Pet {
  name: String
  nickname: String
}

type Dog implements Pet {
  name: String
  nickname: String
  barkVolume: Int
}

type Cat implements Pet {
  name: String
  nickname: String
  meowVolume: Int
}
"#;
    let mut compiler = ApolloCompiler::new();
    let id = compiler.add_document(input, "doc.graphql");

    let schema = compiler.db.schema();
    let diagnostics = compiler
        .db
        .executable_document(id)
        .validate(&schema)
        .unwrap_err();
    let diagnostics = format!("{diagnostics:#}");
    assert!(
        diagnostics.contains("the operation `getName` is defined multiple times in the document"),
        "{diagnostics}"
    );
}

#[test]
fn it_validates_unique_operation_names() {
    let input = r#"
query getCatName {
  cat {
    name
  }
}

query getPetNickname {
  cat {
    nickname
  }
}

type Query {
  cat: Pet
}

union CatOrDog = Cat | Dog

interface Pet {
  name: String
  nickname: String
}

type Dog implements Pet {
  name: String
  nickname: String
  barkVolume: Int
}

type Cat implements Pet {
  name: String
  nickname: String
  meowVolume: Int
}
"#;
    let mut compiler = ApolloCompiler::new();
    compiler.add_document(input, "schema.graphql");

    let diagnostics = compiler.validate();
    for diagnostic in &diagnostics {
        println!("{diagnostic}")
    }
    assert!(diagnostics.is_empty());
}

#[test]
fn it_raises_an_error_for_illegal_operations() {
    let input = r#"
subscription sub {
  newMessage {
    body
    sender
  }
}

type Query {
  cat: Pet
}

union CatOrDog = Cat | Dog

interface Pet {
  name: String
}

type Dog implements Pet {
  name: String
  nickname: String
  barkVolume: Int
}

type Cat implements Pet {
  name: String
  nickname: String
  meowVolume: Int
}
"#;
    let mut compiler = ApolloCompiler::new();
    let id = compiler.add_document(input, "doc.graphql");

    let schema = compiler.db.schema();
    let diagnostics = compiler
        .db
        .executable_document(id)
        .validate(&schema)
        .unwrap_err();
    let diagnostics = format!("{diagnostics:#}");
    assert!(
        diagnostics
            .contains("`subscription` is not defined in the schema and is therefore not supported"),
        "{diagnostics}"
    );
}

#[test]
fn it_validates_fields_in_operations() {
    let input = r#"
query getProduct {
  name
  noName
  topProducts {
    inStock
    price
  }
}

type Query {
  name: String
  topProducts: Product
}

type Product {
  inStock: Boolean
  name: String
}
"#;

    let mut compiler = ApolloCompiler::new();
    compiler.add_document(input, "schema.graphql");

    let diagnostics = compiler.validate();
    for diagnostic in &diagnostics {
        println!("{diagnostic}");
    }

    assert_eq!(diagnostics.len(), 2)
}
