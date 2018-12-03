extern crate custom_error;
extern crate goblin;
extern crate regex;
extern crate yaml_rust;

use itertools::Itertools;
use std::io::Read;

pub struct FilteredSymbols {
    export: Vec<String>,
    strip: Vec<String>,
}

impl FilteredSymbols {
    fn new() -> Self {
        FilteredSymbols {
            export: Vec::new(),
            strip: Vec::new(),
        }
    }

    fn merge(&mut self, other: FilteredSymbols) {
        self.export.extend(other.export);
        self.strip.extend(other.strip);
    }
}

impl std::fmt::Display for FilteredSymbols {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if !self.export.is_empty() {
            writeln!(f, "Exported:");
            for name in &self.export {
                writeln!(f, "\t{}", name);
            }
        }
        if !self.strip.is_empty() {
            writeln!(f, "Stripped:");
            for name in &self.strip {
                writeln!(f, "\t{}", name);
            }
        }
        Ok(())
    }
}

pub trait Filter {
    fn filter(&self, Vec<String>) -> FilteredSymbols;

    fn run_from_bytes(&self, bytes: &[u8]) -> goblin::error::Result<FilteredSymbols> {
        match goblin::Object::parse(&bytes)? {
            goblin::Object::Elf(elf) => {
                // Undefined symbols need to be exported for dynamic linking
                let (undefined_symbols, defined_symbols): (Vec<_>, Vec<_>) = elf
                    .syms
                    .iter()
                    .filter(|x| &elf.strtab[x.st_name] != "")
                    .partition_map(|x| {
                        let symbol_name = String::from(&elf.strtab[x.st_name]);
                        if x.st_value == 0 {
                            itertools::Either::Left(symbol_name)
                        } else {
                            itertools::Either::Right(symbol_name)
                        }
                    });
                let mut symbols = FilteredSymbols {
                    export: undefined_symbols,
                    strip: Vec::new(),
                };

                symbols.merge(self.filter(defined_symbols));
                Ok(symbols)
            }
            goblin::Object::Mach(mach) => {
                match mach {
                    goblin::mach::Mach::Fat(_fat) => Ok(FilteredSymbols::new()),
                    goblin::mach::Mach::Binary(macho) => {
                        // Undefined symbols need to be exported for dynamic linking
                        let (undefined_symbols, defined_symbols): (
                            Vec<_>,
                            Vec<_>,
                        ) = macho
                            .symbols()
                            .filter_map(|x| x.ok())
                            .filter(|x| x.0 != "")
                            .partition_map(|x| {
                                let symbol_name = x.0.to_string();
                                if x.1.is_undefined() {
                                    itertools::Either::Left(symbol_name)
                                } else {
                                    itertools::Either::Right(symbol_name)
                                }
                            });
                        let mut symbols = FilteredSymbols {
                            export: undefined_symbols,
                            strip: Vec::new(),
                        };

                        symbols.merge(self.filter(defined_symbols));
                        Ok(symbols)
                    }
                }
            }
            goblin::Object::Archive(archive) => {
                let mut symbols = FilteredSymbols::new();
                for member in archive.members() {
                    println!("Parsing archive member: {}", member);
                    symbols.merge(self.run_from_bytes(archive.extract(member, &bytes)?)?);
                }
                Ok(symbols)
            }
            _ => Ok(FilteredSymbols::new()),
        }
    }

    fn run_from_file(&self, path_arg: &str) -> goblin::error::Result<FilteredSymbols> {
        let path = std::path::Path::new(path_arg);
        let mut fd = std::fs::File::open(path)?;
        let mut buffer = Vec::new();
        fd.read_to_end(&mut buffer)?;
        let filtered_symbols = self.run_from_bytes(&buffer)?;
        Ok(filtered_symbols)
    }
}

pub struct Export;
impl Export {
    pub fn new() -> Self {
        Export
    }
}
impl Filter for Export {
    fn filter(&self, symbols: Vec<String>) -> FilteredSymbols {
        FilteredSymbols {
            export: symbols,
            strip: Vec::new(),
        }
    }
}

pub struct Strip;
impl Strip {
    pub fn new() -> Self {
        Strip
    }
}
impl Filter for Strip {
    fn filter(&self, symbols: Vec<String>) -> FilteredSymbols {
        FilteredSymbols {
            export: Vec::new(),
            strip: symbols,
        }
    }
}

fn yaml_string(x: &str) -> yaml_rust::Yaml {
    yaml_rust::Yaml::String(x.to_string())
}

custom_error::custom_error!{pub RulesError
    FileError{source: std::io::Error}                 = "could not open configuration file",
    ParseError{source: yaml_rust::scanner::ScanError} = "could not parse configuration file as YAML",
    ConfigurationError                                = "configuration is not valid",
    RegexError{source: regex::Error}                  = "invalid regex",
}

pub struct Rules {
    export_matching: bool,
    regex: regex::RegexSet,
}

fn rules_to_regex(rules: &yaml_rust::Yaml) -> Result<regex::RegexSet, RulesError> {
    let mut re: Vec<String> = Vec::new();
    let rules_map = rules.as_hash().ok_or(RulesError::ConfigurationError)?;

    if let Some(regex_rules) = rules_map.get(&yaml_string("regex")) {
        match regex_rules {
            yaml_rust::Yaml::String(string) => re.push(string.to_string()),
            yaml_rust::Yaml::Array(array) => {
                for string in array {
                    re.push(
                        string
                            .as_str()
                            .ok_or(RulesError::ConfigurationError)?
                            .to_string(),
                    );
                }
            }
            _ => return Err(RulesError::ConfigurationError),
        };
    };

    if let Some(exact_rules) = rules_map.get(&yaml_string("exact")) {
        match exact_rules {
            yaml_rust::Yaml::String(string) => re.push(format!("^{}$", string)),
            yaml_rust::Yaml::Array(array) => {
                for string in array {
                    re.push(format!(
                        "^{}$",
                        string
                            .as_str()
                            .ok_or(RulesError::ConfigurationError)?
                            .to_string(),
                    ));
                }
            }
            _ => return Err(RulesError::ConfigurationError),
        }
    }

    if re.is_empty() {
        Err(RulesError::ConfigurationError)
    } else {
        Ok(regex::RegexSet::new(re)?)
    }
}

impl Rules {
    pub fn new(config_path: &str) -> Result<Self, RulesError> {
        let path = std::path::Path::new(config_path);
        let contents = std::fs::read_to_string(path)?;
        let mut config = yaml_rust::YamlLoader::load_from_str(&contents)?;
        if config.len() != 1 {
            return Err(RulesError::ConfigurationError);
        }
        match config.pop().ok_or(RulesError::ConfigurationError)? {
            yaml_rust::Yaml::Hash(hash) => {
                let export_matching = hash
                    .get(&yaml_string("export_matching"))
                    .unwrap_or(&yaml_rust::Yaml::Boolean(true)) // default to true
                    .as_bool()
                    .ok_or(RulesError::ConfigurationError)?;

                Ok(Rules {
                    export_matching: export_matching,
                    regex: rules_to_regex(
                        hash.get(&yaml_string("rules"))
                            .ok_or(RulesError::ConfigurationError)?, // rules must be present
                    )?,
                })
            }
            _ => Err(RulesError::ConfigurationError),
        }
    }
}
impl Filter for Rules {
    fn filter(&self, mut symbols: Vec<String>) -> FilteredSymbols {
        let (mut matched, mut not_matched) = (Vec::new(), Vec::new());
        loop {
            if let Some(symbol) = symbols.pop() {
                if self.regex.is_match(&symbol) {
                    matched.push(symbol);
                } else {
                    not_matched.push(symbol);
                }
            } else {
                break;
            }
        }
        if self.export_matching {
            FilteredSymbols {
                export: matched,
                strip: not_matched,
            }
        } else {
            FilteredSymbols {
                export: not_matched,
                strip: matched,
            }
        }
    }
}
