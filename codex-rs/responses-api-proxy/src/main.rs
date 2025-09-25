use clap::Parser;
use codex_arg0::PreMainArgs;
use codex_arg0::arg0_dispatch_or_else;
use codex_responses_api_proxy::Args as ResponsesApiProxyArgs;

pub fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(true, |pre_main_args| async move {
        let PreMainArgs {
            codex_linux_sandbox_exe: _,
            openai_api_key,
        } = pre_main_args;

        let openai_api_key =
            openai_api_key.ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY must be set"))?;

        let args = ResponsesApiProxyArgs::parse();
        codex_responses_api_proxy::run_main(openai_api_key, args)
    })
}
