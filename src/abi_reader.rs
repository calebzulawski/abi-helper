extern crate goblin;

use std::io::Read;

pub struct FilteredSymbols {
    export: Vec<String>,
    strip: Vec<String>,
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

    fn run(&self, path_arg: &str) -> goblin::error::Result<()> {
        println!("Parsing {}", path_arg);
        let path = std::path::Path::new(path_arg);
        let mut fd = std::fs::File::open(path)?;
        let mut buffer = Vec::new();
        fd.read_to_end(&mut buffer)?;
        let symbols = match goblin::Object::parse(&buffer)? {
            goblin::Object::Elf(elf) => elf
                .syms
                .iter()
                .filter(|x| {
                    x.st_value != 0 && match goblin::elf::sym::st_type(x.st_info) {
                        goblin::elf::sym::STT_SECTION => false,
                        _ => true,
                    }
                }).map(|x| String::from(&elf.strtab[x.st_name]))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let filtered_symbols = Self::filter(&symbols);
        print!("{}", filtered_symbols);
        Ok(())
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
