use chrono::Datelike;
use chrono::{Local, NaiveDate};
use clap::Parser;
use color_eyre::eyre::{bail, Result};
use directories::ProjectDirs;
use serde::Deserialize;
use std::fmt;
use std::path::Path;
use std::str;

const THIS_PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Copy, Clone)]
enum Verbosity {
    Quiet = 1,
    Normal = 2,
    Verbose = 3,
}

#[derive(Debug)]
enum DateSpecifier {
    Full(String),
    YearOnly(String),
}

impl DateSpecifier {
    fn year(year: &str) -> Self {
        Self::YearOnly(year.to_owned())
    }

    fn full(date: &str) -> Self {
        Self::Full(date.to_owned())
    }

    fn to_naive_date(&self, format_str: &str) -> Result<NaiveDate> {
        let date = match self {
            DateSpecifier::Full(date) => NaiveDate::parse_from_str(&date, &format_str)?,
            DateSpecifier::YearOnly(year) => {
                let year = year.parse::<i32>()?;
                // Default to January 1st for evaluation purposes
                NaiveDate::from_yo_opt(year, 1)
                    .ok_or_else(|| format!("Invalid year: {}", year))
                    .unwrap()
            }
        };
        Ok(date)
    }

    fn is_full(&self) -> bool {
        match self {
            DateSpecifier::Full(_) => true,
            DateSpecifier::YearOnly(_) => false,
        }
    }
}

#[derive(Debug, Deserialize)]
enum DateFormat {
    /// Month, day, year
    MDY { separator: char },
    /// Day, month, year
    DMY { separator: char },
    /// Year, month, day
    YMD { separator: char },
}

impl Default for DateFormat {
    fn default() -> Self {
        DateFormat::MDY { separator: '/' }
    }
}

impl fmt::Display for DateFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DateFormat::MDY { separator } => write!(
                f,
                "%m{}%d{}%Y",
                separator.to_string(),
                separator.to_string()
            ),
            DateFormat::DMY { separator } => write!(
                f,
                "%d{}%m{}%Y",
                separator.to_string(),
                separator.to_string()
            ),
            DateFormat::YMD { separator } => write!(
                f,
                "%Y{}%m{}%d",
                separator.to_string(),
                separator.to_string()
            ),
        }
    }
}

impl str::FromStr for DateFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        let first = chars.next().ok_or("No first character found")?;
        let second = chars.next().ok_or("No second character found")?;
        let third = chars.next().ok_or("No third character found")?;
        let separator = chars.next().ok_or("No separator found")?;

        let format = match (first, second, third) {
            ('M', 'D', 'Y') => DateFormat::MDY { separator },
            ('D', 'M', 'Y') => DateFormat::DMY { separator },
            ('Y', 'M', 'D') => DateFormat::YMD { separator },
            _ => return Err("Invalid date format".to_owned()),
        };
        Ok(format)
    }
}

impl DateFormat {
    fn as_fmt_string(&self) -> String {
        match self {
            DateFormat::MDY { separator } => {
                format!("%m{}%d{}%Y", separator.to_string(), separator.to_string())
            }
            DateFormat::DMY { separator } => {
                format!("%d{}%m{}%Y", separator.to_string(), separator.to_string())
            }
            DateFormat::YMD { separator } => {
                format!("%Y{}%m{}%d", separator.to_string(), separator.to_string())
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    birthday: Option<String>,
    birthyear: Option<String>,
    format: Option<DateFormat>,
}

impl ConfigFile {
    fn from_file(path: &Path) -> Result<Self> {
        let contents = ::std::fs::read_to_string(path)?;
        let config: ConfigFile = toml::from_str(&contents)?;
        Ok(config)
    }
}

#[derive(Debug)]
struct App {
    birthday: NaiveDate,
    current_date: NaiveDate,
    verbosity: Verbosity,
    wish_happy_birthday: bool,
}

impl App {
    fn calculate(&self) -> u32 {
        let current_date = self.current_date;
        let birthday = self.birthday;

        if self.verbosity == Verbosity::Verbose {
            println!("Current date: {:?}", current_date);
            println!("Birthday: {:?}", birthday);
        }

        if self.wish_happy_birthday
            && self.verbosity >= Verbosity::Normal
            && current_date.month() == birthday.month()
            && current_date.day() == birthday.day()
        {
            println!("Happy birthday!");
        }

        let age = current_date.years_since(birthday).unwrap();
        age
    }
}

#[derive(Debug)]
struct LayeredAppConfigBuilder {
    birthday: Option<DateSpecifier>,
    current_date: Option<DateSpecifier>,
    format: DateFormat,
    verbosity: Verbosity,
}

impl LayeredAppConfigBuilder {
    fn new() -> Self {
        Self {
            birthday: None,
            current_date: None,
            format: DateFormat::default(),
            verbosity: Verbosity::Normal,
        }
    }

    fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = verbosity;
        self
    }

    fn stack_args_layer(mut self, args: &Args) -> Self {
        if let Some(birthday) = &args.birthday {
            self.birthday = Some(DateSpecifier::full(birthday));
        } else if let Some(birthyear) = &args.birthyear {
            self.birthday = Some(DateSpecifier::year(birthyear));
        }

        if let Some(date) = &args.date {
            self.current_date = Some(DateSpecifier::full(date));
        } else if let Some(year) = &args.year {
            self.current_date = Some(DateSpecifier::year(year));
        }

        if let Some(format) = &args.format {
            self.format = format.parse().unwrap();
        }
        self
    }

    fn stack_file_layer(mut self, path: &Path) -> Self {
        let Ok(config) = ConfigFile::from_file(path) else {
            if self.verbosity == Verbosity::Verbose {
                eprintln!("Could not read config file");
            }
            return self;
        };

        // Redundant if both are set - birthday takes precedence
        if let Some(birthday) = config.birthday {
            self.birthday = Some(DateSpecifier::full(&birthday));
        } else if let Some(birthyear) = config.birthyear {
            self.birthday = Some(DateSpecifier::year(&birthyear));
        }

        if let Some(format) = config.format {
            self.format = format;
        }
        self
    }

    fn build(&self) -> Result<App> {
        let format = self.format.as_fmt_string();

        let Some(birthday) = &self.birthday else {
            bail!("No birthday specified in either config or command line args");
        };

        let mut wish_happy_birthday = birthday.is_full();
        let birthday = birthday.to_naive_date(&format)?;

        let current_date = if let Some(current_date) = &self.current_date {
            current_date.to_naive_date(&format)?
        } else {
            wish_happy_birthday = false;
            let current_date = Local::now().naive_local().date();
            current_date
        };

        let verbosity = self.verbosity;

        Ok(App {
            birthday,
            current_date,
            verbosity,
            wish_happy_birthday,
        })
    }
}

#[derive(Debug, Parser)]
struct Args {
    /// Increase message verbosity
    #[clap(short, long, group = "verbosity")]
    verbose: bool,

    /// Silence all output except the user's age
    #[clap(short, long, group = "verbosity")]
    quiet: bool,

    /// Override today's date
    #[clap(short, long, group = "current_date")]
    date: Option<String>,

    /// Override today's date, but just the year
    #[clap(short, long, group = "current_date")]
    year: Option<String>,

    /// Specify your birthday
    #[clap(short, long, group = "birthday_specifier")]
    birthday: Option<String>,

    /// Specify just your birth year
    #[clap(long, group = "birthday_specifier")]
    birthyear: Option<String>,

    /// Datetime format
    #[clap(short, long)]
    format: Option<String>,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Args::parse();

    let verbosity = if args.verbose {
        Verbosity::Verbose
    } else if args.quiet {
        Verbosity::Quiet
    } else {
        Verbosity::Normal
    };

    let mut config_builder = LayeredAppConfigBuilder::new().verbosity(verbosity);
    if let Some(proj_dirs) = ProjectDirs::from("", "", THIS_PROGRAM_NAME) {
        let config_dir = proj_dirs.config_dir();
        let config_file = config_dir.join("config.toml");
        config_builder = config_builder.stack_file_layer(&config_file);
    }
    config_builder = config_builder.stack_args_layer(&args);

    let app = config_builder.build()?;
    let age = app.calculate();
    println!("{}", age);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn on_actual_birthday() {
        let birthyear = 1998;
        let diff = 0;
        let currentyear = birthyear + diff;
        let app = App {
            birthday: NaiveDate::from_ymd_opt(birthyear, 1, 1).unwrap(),
            current_date: NaiveDate::from_ymd_opt(currentyear, 1, 1).unwrap(),
            wish_happy_birthday: false,
            verbosity: Verbosity::Normal,
        };
        let age = app.calculate();
        assert_eq!(age, diff as u32);
    }

    #[test]
    fn on_birthday() {
        let birthyear = 1998;
        let diff = 26;
        let currentyear = birthyear + diff;
        let app = App {
            birthday: NaiveDate::from_ymd_opt(birthyear, 1, 1).unwrap(),
            current_date: NaiveDate::from_ymd_opt(currentyear, 1, 1).unwrap(),
            wish_happy_birthday: false,
            verbosity: Verbosity::Normal,
        };
        let age = app.calculate();
        assert_eq!(age, diff as u32);
    }

    #[test]
    fn on_birthday_really_young() {
        let birthyear = 1998;
        let diff = 1;
        let currentyear = birthyear + diff;
        let app = App {
            birthday: NaiveDate::from_ymd_opt(birthyear, 1, 1).unwrap(),
            current_date: NaiveDate::from_ymd_opt(currentyear, 1, 1).unwrap(),
            wish_happy_birthday: false,
            verbosity: Verbosity::Normal,
        };
        let age = app.calculate();
        assert_eq!(age, diff as u32);
    }

    #[test]
    fn on_birthday_really_old() {
        let birthyear = 1998;
        let diff = 1000;
        let currentyear = birthyear + diff;
        let app = App {
            birthday: NaiveDate::from_ymd_opt(birthyear, 1, 1).unwrap(),
            current_date: NaiveDate::from_ymd_opt(currentyear, 1, 1).unwrap(),
            wish_happy_birthday: false,
            verbosity: Verbosity::Normal,
        };
        let age = app.calculate();
        assert_eq!(age, diff as u32);
    }

    #[test]
    fn day_before_birthday() {
        let birthyear = 1998;
        let diff = 26;
        let currentyear = birthyear + diff;
        let app = App {
            birthday: NaiveDate::from_ymd_opt(birthyear, 1, 2).unwrap(),
            current_date: NaiveDate::from_ymd_opt(currentyear, 1, 1).unwrap(),
            wish_happy_birthday: false,
            verbosity: Verbosity::Normal,
        };
        let age = app.calculate();
        assert_eq!(age, diff as u32 - 1);
    }

    #[test]
    fn day_after_birthday() {
        let birthyear = 1998;
        let diff = 26;
        let currentyear = birthyear + diff;
        let app = App {
            birthday: NaiveDate::from_ymd_opt(birthyear, 1, 1).unwrap(),
            current_date: NaiveDate::from_ymd_opt(currentyear, 1, 2).unwrap(),
            wish_happy_birthday: false,
            verbosity: Verbosity::Normal,
        };
        let age = app.calculate();
        assert_eq!(age, diff as u32);
    }
}
