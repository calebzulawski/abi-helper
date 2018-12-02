extern crate goblin;

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
    fn filter(&Vec<String>) -> FilteredSymbols;

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

                symbols.merge(Self::filter(&defined_symbols));
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

                        symbols.merge(Self::filter(&defined_symbols));
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
    fn filter(symbols: &Vec<String>) -> FilteredSymbols {
        FilteredSymbols {
            export: symbols.clone(),
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
    fn filter(symbols: &Vec<String>) -> FilteredSymbols {
        FilteredSymbols {
            export: Vec::new(),
            strip: symbols.clone(),
        }
    }
}
