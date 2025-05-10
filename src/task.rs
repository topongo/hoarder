use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
pub(crate) struct ShellTask {
    args: Vec<String>,
}

impl ShellTask {
    fn new(args: Vec<impl ToString>) -> Self {
        ShellTask { args: args.into_iter().map(|arg| arg.to_string()).collect() }
    }

    pub(crate) fn autosplit(args: impl ToString) -> Self {
        let args = args.to_string();
        if args.contains('"') {
            panic!("autosplit can't be used on a string containing quoted arguments!");
        }
        Self {
            args: args.split_whitespace().map(|arg| arg.to_string()).collect(),
        }
    }

    pub(crate) fn args(&self) -> impl Iterator<Item = &str> {
        self.args.iter().map(|arg| arg.as_str())
    }
}


