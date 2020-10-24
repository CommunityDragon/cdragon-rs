use clap::{App, SubCommand, ArgMatches};
use cdragon_utils::Result;


/// Wrap clap commands to group declaration, argument matching and handling
pub struct NestedCommand<'a> {
    name: &'static str,
    options: Option<Box<dyn for<'x, 'y> Fn(App<'x, 'y>) -> App<'x, 'y> + 'a>>,
    runner: Option<Box<dyn Fn(&ArgMatches<'_>) -> Result<()> + 'a>>,
    nested: Vec<NestedCommand<'a>>,
}

impl<'a> NestedCommand<'a> {
    pub fn new(name: &'static str) -> Self {
        Self { name, options: None, runner: None, nested: vec![] }
    }

    /// Set options on the wrapped App
    pub fn options(mut self, f: impl for<'x, 'y> Fn(App<'x, 'y>) -> App<'x, 'y> + 'a) -> Self {
        self.options = Some(Box::new(f));
        self
    }

    /// Set runner for the wrapped App
    pub fn runner(mut self, f: impl Fn(&ArgMatches<'_>) -> Result<()> + 'a) -> Self {
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

    fn create_app<'x, 'y>(&self) -> App<'x, 'y> {
        let mut app = SubCommand::with_name(self.name);
        if let Some(f) = self.options.as_ref() {
            app = f(app);
        }
        self.nested.iter().fold(app, |app, sub| app.subcommand(sub.create_app()))
    }

    fn run_with_matches(self, appm: &ArgMatches<'_>) -> Result<()> {
        if let Some(f) = self.runner {
            f(appm)?;
        }
        if self.nested.is_empty() {
            Ok(())  // nothing to do
        } else if let (subname, Some(subm)) = appm.subcommand() {
            let sub = self.nested.into_iter()
                .filter(|cmd| cmd.name == subname)
                .next()
                .expect("undeclared subcommand");
            sub.run_with_matches(subm)
        } else {
            panic!("missing subcommand");
        }
    }
}


