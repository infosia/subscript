//! Source programs for the rejection-site witness table.

use crate::{CheckOptions, SourceFile};

pub(super) struct Program {
    pub(super) key: String,
    pub(super) codes: std::collections::BTreeSet<String>,
    pub(super) files: Vec<SourceFile>,
    pub(super) poison: Vec<String>,
    pub(super) runner: bool,
    pub(super) enabled_modules: Vec<String>,
}

pub(super) fn programs() -> Vec<Program> {
    include_str!("rejection_programs.txt")
        .split("END PROGRAM\n")
        .filter(|record| !record.is_empty())
        .map(|record| {
            let (header, body) = record.split_once('\n').unwrap();
            let fields: Vec<_> = header.split('\t').collect();
            let files = body
                .split("END FILE\n")
                .filter(|record| !record.is_empty())
                .map(|record| {
                    let (header, source) = record.split_once('\n').unwrap();
                    let fields: Vec<_> = header.split('\t').collect();
                    match fields[1] {
                        "A" => SourceFile::ambient(fields[2], source),
                        "E" => SourceFile::entry(fields[2], source),
                        "N" => SourceFile::new(fields[2], source),
                        other => panic!("unknown input role {other}"),
                    }
                })
                .collect();
            Program {
                key: fields[1].to_owned(),
                codes: fields[2]
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                poison: fields[3]
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                runner: fields[4] == "1",
                enabled_modules: fields
                    .iter()
                    .skip(5)
                    .filter_map(|field| field.strip_prefix("enable-module="))
                    .flat_map(|modules| modules.split(","))
                    .filter(|module| !module.is_empty())
                    .map(str::to_owned)
                    .collect(),
                files,
            }
        })
        .collect()
}

pub(super) fn check(program: &Program) -> Vec<crate::Diagnostic> {
    let options = CheckOptions {
        poison_missing_modules: program.poison.clone(),
        enabled_modules: program.enabled_modules.clone(),
        ..CheckOptions::default()
    };
    match crate::check_program_with(&program.files, &options) {
        Ok(module) if program.runner => module.runner_main().err().into_iter().collect(),
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics,
    }
}

pub(super) fn write_tsc(program: &Program, temporary: &std::path::Path) -> Vec<std::path::PathBuf> {
    let directory = temporary.join("general").join(&program.key);
    std::fs::create_dir_all(&directory).unwrap();
    let ambient = program
        .files
        .iter()
        .filter(|file| file.dts && !file.source.contains("declare module"))
        .map(|file| file.source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    program
        .files
        .iter()
        .filter(|file| !file.dts || file.source.contains("declare module"))
        .map(|file| {
            let path = directory.join(std::path::Path::new(&file.name).file_name().unwrap());
            let source = if file.entry {
                format!("{ambient}\n{}", file.source)
            } else {
                file.source.clone()
            };
            std::fs::write(&path, source).unwrap();
            path
        })
        .collect()
}
