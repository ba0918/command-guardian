use crate::{Ask, Effect};

/// 見かけの語と、本文にある先頭代入の名前。環境展開したパスではない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub words: Vec<String>,
    pub env_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandFacts {
    pub effects: Vec<Effect>,
    pub invocations: Vec<Invocation>,
    pub diagnostics: Vec<Ask>,
}
