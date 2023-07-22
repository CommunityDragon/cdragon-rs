use clap::{Command, ArgMatches};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;


/// Wrap clap commands to group declaration, argument matching and handling
pub struct NestedCommand<'a> {
    name: &'static str,
    options: Option<Box<dyn Fn(Command) -> Command + 'a>>,
    runner: Option<Box<dyn Fn(&ArgMatches) -> Result<()> + 'a>>,
    nested: Vec<NestedCommand<'a>>,
}

impl<'a> NestedCommand<'a> {
    pub fn new(name: &'static str) -> Self {
        Self { name, options: None, runner: None, nested: vec![] }
    }

    /// Set options on the wrapped App
    pub fn options(mut self, f: impl Fn(Command) -> Command + 'a) -> Self {
        self.options = Some(Box::new(f));
        self
    }

    /// Set runner for the wrapped App
    pub fn runner(mut self, f: impl Fn(&ArgMatches) -> Result<()> + 'a) -> Self {
        self.runner = Some(Box::new(f));
        self
    }

    /// Add a nested subcommand
    pub fn add_nested(mut self, cmd: Self) -> Self {
        self.nested.push(cmd);
        self
    }

    /// Run the command, match nested subcommands
    pub fn run(self) -> Result<()> {
        let appm = self.create_app().get_matches();
        self.run_with_matches(&appm)
    }

    fn create_app(&self) -> Command {
        let mut app = Command::new(self.name);
        if let Some(f) = self.options.as_ref() {
            app = f(app);
        }
        self.nested.iter().fold(app, |app, sub| app.subcommand(sub.create_app()))
    }

    fn run_with_matches(self, appm: &ArgMatches) -> Result<()> {
        if let Some(f) = self.runner {
            f(appm)?;
        }
        if self.nested.is_empty() {
            Ok(())  // nothing to do
        } else if let Some((subname, subm)) = appm.subcommand() {
            let sub = self.nested.into_iter()
                .find(|cmd| cmd.name == subname)
                .expect("undeclared subcommand");
            sub.run_with_matches(subm)
        } else {
            panic!("missing subcommand");
        }
    }
}


