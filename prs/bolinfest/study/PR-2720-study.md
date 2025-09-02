**DOs**
- Define globally + enable by ID: Keep MCP server definitions in one global table; let profiles list which IDs to enable.
```toml
# Global definitions
[mcp_servers.s3]
command = "s3-mcp"

[mcp_servers.analytics]
command = "analytics-mcp"

# Profile selects by ID
[profiles.ops]
enabled_mcp_servers = ["s3", "analytics"]
```

- Allow per-profile definitions: Let profiles add or override servers for their own use.
```toml
# Profile adds a server not in globals
[profiles.ops.mcp_servers.internals]
command = "internal-mcp"

# Profile-specific override of a global server
[profiles.ops.mcp_servers.analytics]
env = { REGION = "us-west-2" }  # overrides/extends global
```

- Support both modalities together: Profiles can both enable global servers by ID and define/override per-profile servers.
```toml
[mcp_servers.git]
command = "git-mcp"

[profiles.ops]
enabled_mcp_servers = ["git", "internals"]  # includes global and profile-defined
[profiles.ops.mcp_servers.internals]
command = "internal-mcp"
```

- Make inheritance explicit: Provide a profile-level switch to include or exclude globals; be clear in docs.
```toml
# Opt-out: only what the profile enables/defines
[profiles.minimal]
inherit_global_mcp_servers = false
enabled_mcp_servers = []  # explicit: no servers
```

- Offer a default-profile opt-out per server: Let a server be defined globally but not auto-enabled in the default profile.
```toml
[mcp_servers.heavy]
command = "heavy-mcp"
disable_in_default_profile = true  # only enabled when a profile names it

[profiles.ops]
enabled_mcp_servers = ["heavy"]  # explicitly opt-in
```

- Document precedence clearly: On key conflicts, the profile’s `mcp_servers.<id>` overrides the global `<id>`.
```toml
[mcp_servers.analytics]
command = "analytics-mcp"
env = { REGION = "us-east-1" }

[profiles.ops.mcp_servers.analytics]
env = { REGION = "us-west-2" }  # profile takes precedence
```

**DON'Ts**
- Define servers outside the `mcp_servers` table under profiles: Avoid ambiguous tables like `[profiles.<name>.<id>]`.
```toml
# Bad (ambiguous)
[profiles.ops.analysis]
command = "ops-mcp"

# Good (scoped correctly)
[profiles.ops.mcp_servers.analysis]
command = "ops-mcp"
```

- Rely on implicit inheritance for safety-sensitive setups: Be explicit when a profile should not see globals.
```toml
# Risky: inherits all globals by default (could expose sensitive servers)
[profiles.audited]
# inherit_global_mcp_servers implicitly true

# Safer: opt out, then opt in
[profiles.audited]
inherit_global_mcp_servers = false
enabled_mcp_servers = ["s3"]  # only what’s allowed
```

- Enable sensitive servers in default unintentionally: Use server-level opt-out and profile opt-in instead.
```toml
# Bad: heavy is globally defined and implicitly available to default
[mcp_servers.heavy]
command = "heavy-mcp"

# Good: hide from default; enable only where intended
[mcp_servers.heavy]
command = "heavy-mcp"
disable_in_default_profile = true

[profiles.ops]
enabled_mcp_servers = ["heavy"]
```

- Create conflicting IDs without intent: Avoid accidental ID collisions between global and profile servers.
```toml
# Bad: accidental conflict; behavior unclear
[mcp_servers.reports]        # global
command = "reports-mcp"

[profiles.ops.mcp_servers.reports]  # profile reuses ID unintentionally

# Good: either rename or override intentionally (and test)
[profiles.ops.mcp_servers.reports]
command = "reports-mcp"  # intentional override of global
```

- Publish docs with mismatched paths/examples: Keep examples consistent with the schema you support.
```toml
# Wrong in docs
[profiles.ops.analysis]

# Right in docs
[profiles.ops.mcp_servers.analysis]
# and/or
[profiles.ops]
enabled_mcp_servers = ["analysis"]
```