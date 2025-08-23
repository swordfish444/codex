use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Expect exactly one argument: the full apply_patch payload.
    let mut args = std::env::args_os();
    // argv[0]
    let _argv0 = args.next();

    let patch_arg = match args.next() {
        Some(arg) => match arg.into_string() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("Error: apply-patch requires a UTF-8 PATCH argument.");
                return ExitCode::from(1);
            }
        },
        None => {
            eprintln!("Usage: apply-patch '<apply_patch_payload>'");
            return ExitCode::from(2);
        }
    };

    // Refuse extra args to avoid ambiguity.
    if args.next().is_some() {
        eprintln!("Error: apply-patch accepts exactly one argument.");
        return ExitCode::from(2);
    }

    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    match codex_apply_patch::apply_patch(&patch_arg, &mut stdout, &mut stderr) {
        Ok(()) => {
            // Flush to ensure output ordering when used in pipelines.
            let _ = stdout.flush();
            ExitCode::from(0)
        }
        Err(_) => ExitCode::from(1),
    }
}
