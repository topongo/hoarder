use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(transparent)]
pub(crate) struct ShellTask {
    _args: Vec<String>,
}

impl ShellTask {
    pub(crate) fn new(initial: impl ToString) -> Self {
        Self { _args: vec![initial.to_string()] }
    }

    pub(crate) fn autosplit(args: impl ToString) -> Self {
        let args = args.to_string();
        if args.contains('"') {
            panic!("autosplit can't be used on a string containing quoted arguments!");
        }
        Self {
            _args: args.split_whitespace().map(|arg| arg.to_string()).collect(),
        }
    }

    pub(crate) fn get_args(&self) -> impl IntoIterator<Item = &str> {
        self._args.iter().map(|arg| arg.as_str())
    }

    pub(crate) fn arg(&mut self, arg: impl ToString) -> &mut Self {
        self._args.push(arg.to_string());
        self
    }

    pub(crate) fn args(&mut self, args: impl IntoIterator<Item = impl ToString>) -> &mut Self {
        self._args.extend(args.into_iter().map(|arg| arg.to_string()));
        self
    }
}
