/// Tokens generated by the lexer.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u16)]
pub enum TokenKind {
    Bang,     // !
    Dollar,   // $
    Spread,   // ...
    Comma,    // ,
    Colon,    // :
    Eq,       // =
    At,       // @
    LParen,   // (
    RParen,   // )
    LBracket, // [
    RBracket, // ]
    LCurly,   // {
    RCurly,   // }
    Pipe,     // |
    On,
    Eof,

    // composite nodes
    Node,
    StringValue,
    Null,
    Boolean,
    Int,
    Float,

    // Root node
    Root,
}

// TODO: remove me
impl From<TokenKind> for rowan::SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        Self(kind as u16)
    }
}
