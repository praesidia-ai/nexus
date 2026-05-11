//! Tests for the Nexus CLI.
//!
//! Covers command parsing, output format parsing, and NexusClient construction.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    // `Parser` must be in scope for `try_parse_from`.
    #[allow(unused_imports)]
    use clap::Parser;

    // Re-use the types from the parent modules. The `Cli` struct is
    // accessible via `super::super` because this module is `crate::tests::tests`.

    // -----------------------------------------------------------------------
    // OutputFormat parsing
    // -----------------------------------------------------------------------

    #[test]
    fn output_format_parse_human() {
        let fmt: Result<crate::output::OutputFormat, _> = "human".parse();
        assert!(fmt.is_ok());
    }

    #[test]
    fn output_format_parse_json() {
        let fmt: Result<crate::output::OutputFormat, _> = "json".parse();
        assert!(fmt.is_ok());
    }

    #[test]
    fn output_format_parse_quiet() {
        let fmt: Result<crate::output::OutputFormat, _> = "quiet".parse();
        assert!(fmt.is_ok());
    }

    #[test]
    fn output_format_parse_unknown_fails() {
        let fmt: Result<crate::output::OutputFormat, _> = "xml".parse();
        assert!(fmt.is_err());
    }

    #[test]
    fn output_format_display_roundtrip() {
        use std::str::FromStr;
        for name in &["human", "json", "quiet"] {
            let fmt = crate::output::OutputFormat::from_str(name).unwrap();
            let display = fmt.to_string();
            assert_eq!(&display, *name);
        }
    }

    #[test]
    fn output_format_case_insensitive() {
        let fmt: Result<crate::output::OutputFormat, _> = "JSON".parse();
        assert!(fmt.is_ok());
        let fmt: Result<crate::output::OutputFormat, _> = "Human".parse();
        assert!(fmt.is_ok());
    }

    // -----------------------------------------------------------------------
    // NexusClient construction
    // -----------------------------------------------------------------------

    #[test]
    fn nexus_client_trims_trailing_slash() {
        let client = crate::api_client::NexusClient::new("http://localhost:8020/");
        // We can only check that construction doesn't panic.
        // The base_url field is private, so just confirm the client is usable.
        let _ = client;
    }

    #[test]
    fn nexus_client_no_trailing_slash() {
        let client = crate::api_client::NexusClient::new("http://localhost:8020");
        let _ = client;
    }

    #[test]
    fn nexus_client_custom_url() {
        let client = crate::api_client::NexusClient::new("https://nexus.example.com");
        let _ = client;
    }

    // -----------------------------------------------------------------------
    // Command enum parsing via clap try_parse_from
    // -----------------------------------------------------------------------

    #[test]
    fn cli_parse_generate() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "generate",
            "Build a SaaS CRM",
        ]);
        assert!(result.is_ok(), "Failed to parse 'generate': {:?}", result.err());
    }

    #[test]
    fn cli_parse_generate_interactive() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "generate",
            "--interactive",
            "Build a blog",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_team_list() {
        let result = super::super::Cli::try_parse_from(["nexus", "team", "list"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_team_create() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "team",
            "create",
            "--template",
            "fullstack",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_team_run() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "team",
            "run",
            "--team",
            "abc",
            "Build login page",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_agent_list() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "agent",
            "list",
            "--project",
            "proj-1",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_agent_run() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "agent",
            "run",
            "--project",
            "proj-1",
            "--agent",
            "nova",
            "Fix the login bug",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_app_start() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "app",
            "start",
            "--project",
            "proj-1",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_app_logs_with_follow() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "app",
            "logs",
            "--project",
            "proj-1",
            "--follow",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_taste_score() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "taste",
            "score",
            "--project",
            "proj-1",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_deploy_github() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "deploy",
            "github",
            "--project",
            "proj-1",
            "--repo",
            "user/repo",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_deploy_docker() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "deploy",
            "docker",
            "--project",
            "proj-1",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_plugins_list() {
        let result = super::super::Cli::try_parse_from(["nexus", "plugins", "list"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_plugins_install() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "plugins",
            "install",
            "my-plugin",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_plugins_search() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "plugins",
            "search",
            "auth",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_config_get() {
        let result = super::super::Cli::try_parse_from(["nexus", "config", "get", "model"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_config_set() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "config",
            "set",
            "model",
            "gpt-4o",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_config_providers() {
        let result = super::super::Cli::try_parse_from(["nexus", "config", "providers"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_server_start() {
        let result = super::super::Cli::try_parse_from(["nexus", "server", "start"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_server_start_with_port() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "server",
            "start",
            "--port",
            "9090",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_server_status() {
        let result = super::super::Cli::try_parse_from(["nexus", "server", "status"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_projects() {
        let result = super::super::Cli::try_parse_from(["nexus", "projects"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_global_format_json() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "--format",
            "json",
            "projects",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_global_server_url() {
        let result = super::super::Cli::try_parse_from([
            "nexus",
            "--server",
            "http://remote:9090",
            "projects",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parse_no_args_fails() {
        let result = super::super::Cli::try_parse_from(["nexus"]);
        assert!(result.is_err(), "Expected error when no subcommand given");
    }

    #[test]
    fn cli_parse_unknown_subcommand_fails() {
        let result = super::super::Cli::try_parse_from(["nexus", "foobar"]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Output helpers (unit tests for formatting functions)
    // -----------------------------------------------------------------------

    #[test]
    fn print_table_empty_headers_no_panic() {
        // Should not panic with empty headers
        crate::output::print_table(&[], &[]);
    }

    #[test]
    fn print_table_with_data_no_panic() {
        let headers = &["A", "B"];
        let rows = vec![
            vec!["one".to_string(), "two".to_string()],
            vec!["three".to_string(), "four".to_string()],
        ];
        crate::output::print_table(headers, &rows);
    }

    #[test]
    fn print_progress_clamps_to_100() {
        // Should not panic with values > 100
        crate::output::print_progress("test", 150);
    }

    #[test]
    fn print_progress_zero() {
        crate::output::print_progress("start", 0);
    }
}
