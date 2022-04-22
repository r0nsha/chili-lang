use crate::vm::Bytecode;
use std::fmt::Display;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Tuple(Vec<Value>),
    Func(Func),
    // ForeignFunc(ForeignFunc),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(v) => *v,
            _ => false,
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Value::Int(v) => format!("int {}", v),
                Value::Bool(v) => format!("bool {}", v),
                Value::Tuple(v) => format!(
                    "({})",
                    v.iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                ),
                Value::Func(func) => format!("fn {}", func.name),
                // Value::ForeignFunc(func) => format!("foreign(\"{}\") func {}", func.lib, func.name),
            }
        )
    }
}

#[derive(Debug, Clone)]
pub struct Func {
    pub name: String,
    pub arg_count: usize,
    pub code: Bytecode,
}