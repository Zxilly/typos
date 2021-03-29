// 2015-edition macros.
#[macro_use]
extern crate clap;

use std::io::Write;

use structopt::StructOpt;

mod args;
use typos_cli::config;
use typos_cli::report;

use proc_exit::WithCodeResultExt;

fn main() {
    human_panic::setup_panic!();
    let result = run();
    proc_exit::exit(result);
}

fn run() -> proc_exit::ExitResult {
    // clap's `get_matches` uses Failure rather than Usage, so bypass it for `get_matches_safe`.
    let args = match args::Args::from_args_safe() {
        Ok(args) => args,
        Err(e) if e.use_stderr() => {
            return Err(proc_exit::Code::USAGE_ERR.with_message(e));
        }
        Err(e) => {
            writeln!(std::io::stdout(), "{}", e)?;
            return proc_exit::Code::SUCCESS.ok();
        }
    };

    init_logging(args.verbose.log_level());

    if let Some(output_path) = args.dump_config.as_ref() {
        run_dump_config(&args, output_path)
    } else {
        run_checks(&args)
    }
}

fn run_dump_config(args: &args::Args, output_path: &std::path::Path) -> proc_exit::ExitResult {
    let global_cwd = std::env::current_dir()?;

    let path = &args.path[0];
    let path = if path == std::path::Path::new("-") {
        path.to_owned()
    } else {
        path.canonicalize().with_code(proc_exit::Code::USAGE_ERR)?
    };
    let cwd = if path == std::path::Path::new("-") {
        global_cwd.as_path()
    } else if path.is_file() {
        path.parent().unwrap()
    } else {
        path.as_path()
    };

    let storage = typos_cli::policy::ConfigStorage::new();
    let mut overrides = config::EngineConfig::default();
    overrides.update(&args.overrides);
    let mut engine = typos_cli::policy::ConfigEngine::new(&storage);
    engine.set_isolated(args.isolated).set_overrides(overrides);
    if let Some(path) = args.custom_config.as_ref() {
        let custom = config::Config::from_file(path).with_code(proc_exit::Code::CONFIG_ERR)?;
        engine.set_custom_config(custom);
    }
    let config = engine
        .load_config(cwd)
        .with_code(proc_exit::Code::CONFIG_ERR)?;

    let mut defaulted_config = config::Config::from_defaults();
    defaulted_config.update(&config);
    let output = toml::to_string_pretty(&defaulted_config).with_code(proc_exit::Code::FAILURE)?;
    if output_path == std::path::Path::new("-") {
        std::io::stdout().write_all(output.as_bytes())?;
    } else {
        std::fs::write(output_path, &output)?;
    }

    Ok(())
}

fn run_checks(args: &args::Args) -> proc_exit::ExitResult {
    let global_cwd = std::env::current_dir()?;

    let mut typos_found = false;
    let mut errors_found = false;
    for path in args.path.iter() {
        let path = if path == std::path::Path::new("-") {
            path.to_owned()
        } else {
            path.canonicalize().with_code(proc_exit::Code::USAGE_ERR)?
        };
        let cwd = if path == std::path::Path::new("-") {
            global_cwd.as_path()
        } else if path.is_file() {
            path.parent().unwrap()
        } else {
            path.as_path()
        };

        let storage = typos_cli::policy::ConfigStorage::new();
        let mut overrides = config::EngineConfig::default();
        overrides.update(&args.overrides);
        let mut engine = typos_cli::policy::ConfigEngine::new(&storage);
        engine.set_isolated(args.isolated).set_overrides(overrides);
        if let Some(path) = args.custom_config.as_ref() {
            let custom = config::Config::from_file(path).with_code(proc_exit::Code::CONFIG_ERR)?;
            engine.set_custom_config(custom);
        }

        engine
            .init_dir(cwd)
            .with_code(proc_exit::Code::CONFIG_ERR)?;
        let files = engine.files(cwd);

        let threads = if path.is_file() { 1 } else { args.threads };
        let single_threaded = threads == 1;

        let mut walk = ignore::WalkBuilder::new(path);
        walk.threads(args.threads)
            .hidden(files.ignore_hidden())
            .ignore(files.ignore_dot())
            .git_global(files.ignore_global())
            .git_ignore(files.ignore_vcs())
            .git_exclude(files.ignore_vcs())
            .parents(files.ignore_parent());

        // HACK: Diff doesn't handle mixing content
        let output_reporter = if args.diff {
            &args::PRINT_SILENT
        } else {
            args.format.reporter()
        };
        let status_reporter = report::MessageStatus::new(output_reporter);
        let reporter: &dyn report::Report = &status_reporter;

        let selected_checks: &dyn typos_cli::file::FileChecker = if args.files {
            &typos_cli::file::FoundFiles
        } else if args.identifiers {
            &typos_cli::file::Identifiers
        } else if args.words {
            &typos_cli::file::Words
        } else if args.write_changes {
            &typos_cli::file::FixTypos
        } else if args.diff {
            &typos_cli::file::DiffTypos
        } else {
            &typos_cli::file::Typos
        };

        if single_threaded {
            typos_cli::file::walk_path(walk.build(), selected_checks, &engine, reporter)
        } else {
            typos_cli::file::walk_path_parallel(
                walk.build_parallel(),
                selected_checks,
                &engine,
                reporter,
            )
        }
        .map_err(|e| {
            e.io_error()
                .map(|i| proc_exit::Code::from(i.kind()))
                .unwrap_or_default()
                .with_message(e)
        })?;
        if status_reporter.typos_found() {
            typos_found = true;
        }
        if status_reporter.errors_found() {
            errors_found = true;
        }
    }

    if errors_found {
        proc_exit::Code::FAILURE.ok()
    } else if typos_found {
        // Can;'t use `Failure` since its so prevalent, it could be easy to get a
        // `Failure` from something else and get it mixed up with typos.
        //
        // Can't use DataErr or anything else an std::io::ErrorKind might map to.
        proc_exit::Code::UNKNOWN.ok()
    } else {
        proc_exit::Code::SUCCESS.ok()
    }
}

fn init_logging(level: Option<log::Level>) {
    if let Some(level) = level {
        let mut builder = env_logger::Builder::new();

        builder.filter(None, level.to_level_filter());

        if level == log::LevelFilter::Trace {
            builder.format_timestamp_secs();
        } else {
            builder.format(|f, record| {
                writeln!(
                    f,
                    "[{}] {}",
                    record.level().to_string().to_lowercase(),
                    record.args()
                )
            });
        }

        builder.init();
    }
}
