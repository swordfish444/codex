/// Trait for writing console messages.
pub trait ConsoleWriter {
    fn exec_command_succeed(&mut self, call_id: &str, truncated_output: &str);
    fn exec_command_fail(&mut self, call_id: &str, exit_code: i32, truncated_output: &str);
}

/// Macro to generate both ANSI and Plain ConsoleWriters
macro_rules! console_writer_impl {
    (
        $StyledWriter:ident, $PlainWriter:ident, $out_field:ident,
        {
            $(
                fn $method:ident(&mut self, $($arg_name:ident: $arg_ty:ty),*) {
                    styled: $styled_fmt:expr,
                    plain: $plain_fmt:expr
                }
            )*
        }
    ) => {
        pub struct $StyledWriter<W: std::io::Write> {
            $out_field: W,
        }

        pub struct $PlainWriter<W: std::io::Write> {
            $out_field: W,
        }

        impl<W: std::io::Write> $StyledWriter<W> {
            pub fn new($out_field: W) -> Self {
                Self { $out_field }
            }
        }

        impl<W: std::io::Write> $PlainWriter<W> {
            pub fn new($out_field: W) -> Self {
                Self { $out_field }
            }
        }

        impl<W: std::io::Write> ConsoleWriter for $StyledWriter<W> {
            $(
                fn $method(&mut self, $($arg_name: $arg_ty),*) {
                    let _ = writeln!(self.$out_field, $styled_fmt, $($arg_name),*);
                }
            )*
        }

        impl<W: std::io::Write> ConsoleWriter for $PlainWriter<W> {
            $(
                fn $method(&mut self, $($arg_name: $arg_ty),*) {
                    let _ = writeln!(self.$out_field, $plain_fmt, $($arg_name),*);
                }
            )*
        }
    };
}

const BOLD_RED: &str = "\x1b[1;31m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

// TODO(mbolin): Escape ANSI codes in plain text output.

console_writer_impl!(
    AnsiConsoleWriter, PlainConsoleWriter, out, {
        fn exec_command_succeed(&mut self, call_id: &str, truncated_output: &str) {
            styled: "{BOLD_GREEN}exec({}) succeeded:{RESET}\n{DIM}{}{RESET}",
            plain: "exec({}) succeeded:\n{}"
        }
        fn exec_command_fail(&mut self, call_id: &str, exit_code: i32, truncated_output: &str) {
            styled: "{BOLD_RED}exec({}) failed ({}):{RESET}\n{DIM}{}{RESET}",
            plain: "exec({}) exited {}:\n{}"
        }
    }
);
