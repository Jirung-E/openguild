# Setup, modes, env vars

## Initialize a guild (once)

```bash
cd /path/to/your-project
openguild init                      # directory name becomes the guild name
openguild init --name "My Project"  # explicit guild name
```

## Modes

**Local (default, recommended)** — no server needed. Auto-discovers `.guild`
from cwd and calls core services directly.

```bash
cd /path/to/your-project
openguild quest list
openguild --guild ./other-project quest list   # explicit guild path
```

**Remote** — HTTP to a hosted server.

```bash
openguild --remote https://host/path quest list
# or
export OPENGUILD_REMOTE=https://host/path
openguild quest list
```

To run your own server:
```bash
GUILD_PATH=/path/to/your-project cargo run --bin openguild-server -- host
# in another terminal:
openguild --remote http://localhost:3000 ping
```

## Env vars / global flags

| Item | Default | Description |
|---|---|---|
| env `OPENGUILD_REMOTE` | (unset) | Remote server URL. Setting it enables remote mode |
| `--remote <URL>` | overrides env | Force remote mode |
| `--guild <PATH>` | (unset) | Local mode's guild path. Auto-discovers from cwd if unset |
| `--json` | off | Machine-readable output (2-space pretty) |
| `--compact` | off | Single-line JSON — pipes/jq/log collection. Requires `--json` |
