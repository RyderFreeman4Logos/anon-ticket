# Config Folder

Keep deployment-specific TOML/JSON/YAML secrets under this directory (e.g.,
`config/api.toml`, `config/monitor.toml`). These files stay out of git thanks
to `.gitignore`; commit only sanitized examples or schema docs. Pair each
secret file with accompanying documentation that explains its keys and
validation rules.
