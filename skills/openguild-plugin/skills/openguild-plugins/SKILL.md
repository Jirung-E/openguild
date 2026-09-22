---
name: openguild-plugins
description: Write an openguild plugin — an event hook that runs the user's own program when something happens in their guild (a quest is created, a status changes, a comment is added), or that blocks a change before it happens. Use when the user asks to get notified about guild activity (Telegram, Slack, Discord, email, a webhook, a desktop notification), to run a script or command on a guild event, to send quest/comment content to an AI or another service, to enforce a rule before a change goes through, or mentions `.guild/plugins`, `plugin.toml`, `openguild plugin`, or "openguild hook". Also use to debug a plugin that is not firing.
---

# Writing an openguild plugin

A plugin calls **the user's own program** when something happens in their guild —
and, in the `pre` phase, can stop the change from happening at all.

```
.guild/plugins/{name}/plugin.toml    what/when/where         (committed to git)
.guild/plugins/{name}/*.rhai         judgement and shaping   (optional, committed)
.guild/plugins/{name}/*.test.rhai    tests for that script   (optional, committed)
.guild/plugins/{name}/anything-else  the program `run` calls (optional, committed)
```

Consent is **not** committed — it lives in `~/.openguild/plugin-consent.json` on
each machine. A plugin arriving by `git pull` does not run until someone here
allows it.

> **Field reference lives in one place.** Every field, every script call and the
> full event vocabulary are in the `openguild` skill's `reference/plugins.md`.
> Read it before writing a definition. This skill is the *procedure* — what to
> decide, in what order, and how to prove the thing works. Don't restate the
> field tables here; they drift.

**A plugin can also live outside the guild** — in a *source*, a folder holding
several plugins (like a marketplace). The guild then uses it in place, without a
copy:

```bash
openguild plugin source add ~/dev/my-plugins   # register the folder (this machine)
openguild plugin add my-hook                   # use it in this guild
```

The folder layout inside a source is the same as `.guild/plugins/`. In the
desktop app the same thing is **Admin → Plugins → + Add plugin** (pick the folder;
a single plugin is used at once, a folder of several is registered and the user
picks from the source list).

## Work in this order

Skipping to the TOML is the usual way to get this wrong. Each step below rules
out a class of plugin that cannot work.

### 1. What should happen, and where?

Ask, or infer from what they said:

- **Before or after.** This is the first fork and it is easy to miss.
  - `post` — react to something that already happened. Almost everything.
  - `pre` — run **before** the change and decide whether it may proceed. Use it
    when the user says "don't let…", "require…", "block…". A `pre` line can
    refuse with a reason or rewrite field values, and it **always waits**, so
    the person feels its cost. Only some events have a `pre` phase; asking for
    one that has none is a load error, not a silent no-op. Check with
    `openguild plugin events`.
- **Where the plugin lives.**
  - `.guild/plugins/` — it belongs to this guild and teammates get it by `git pull`.
  - A **source** folder — it is the user's own (a personal notifier, a backup
    copier) or they want it in several guilds. Nothing is committed; each guild
    that wants it runs `openguild plugin add <name>`. Editing the source changes
    every guild that uses it at once.
- **Which event.** Run `openguild plugin events` for the list — it also prints
  each event's subject kind and the `with` names it allows. If nothing fits, say
  so; inventing a name fails the load, which is better than a plugin that never
  fires, but it still means the request cannot be met as asked.
- **Which machine.** `scope` is required and has no default:
  - `cli` — their terminal. Fires on `openguild quest move …`, batch scripts, CI.
  - `gui` — the desktop app only. Right for anything that interrupts a person
    (a desktop notification on a CLI batch of 20 quests fires 20 times).
  - `server` — applies to **everyone using that server**, not just them.
- **`post` or `run`.** Those two actions are all the core owns. Slack, Discord,
  email, Telegram, an AI call — all of them are a `post` to that service's URL,
  or a `run` of a program the user writes. Do not look for a Slack action;
  there isn't one.

### 2. One line, or several?

A definition is a list of **lines** (`[[handlers]]`), each one "when to look,
what to do". One plugin can hold many, and they run in the order written. Prefer
several small lines over one function with a switch inside — each line carries
its own events, its own condition and its own time budget, and the consent
screen and error messages name them one by one.

```toml
[[handlers]]
    id     = "urgent-quests"
    post   = ["quest.created"]
    action = "out"               # straight to a named destination, no script
    [handlers.when]
        "quest.urgency" = [1, 2]

[[handlers]]
    post = ["comment.added"]
    with = ["subject"]           # fn on_comment(e, subject)
    call = "on_comment"
```

### 3. Does it need a script?

Only write a `.rhai` file if the answer is yes.

- **No script** (`action = …`) → the line fires whenever its events and its
  `when` match, and sends the whole event JSON. Fine for `run`, where the
  program can filter in whatever language it is already written in.
- **`when` only** → conditions on event fields (`"change.to" = ["done"]`,
  `"quest.tags" = "notify"`) need no script at all. Reach for this before rhai.
- **Script** (`call = …`) → needed when the target wants a fixed body shape
  (Telegram wants `{chat_id, text}`), when the decision needs more than field
  equality, or when the line is `pre` and must produce a reason to block.

There is **no I/O in a script** — no files, no network, no processes. It records
work (`send` / `run` / `notify` / `backup`) and the core performs it after the
function returns, outside the guild lock. That is the sandbox, not a gap to work
around.

A `pre` function speaks through its **return value**: a string is the reason to
block, `false` blocks without one, a map is the fields to change, and returning
nothing lets it through.

### 4. Where do the secrets go?

`plugin.toml` is committed. A literal API key in it is in the history forever,
so the loader **rejects** anything that looks like one — the `.rhai` source is
scanned the same way.

| Where the secret goes | How |
|---|---|
| In the URL | `url = "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage"` |
| In a header | `headers = { Authorization = "Bearer ${MY_KEY}" }` |
| In the **body** | `body_env = { chat_id = "TELEGRAM_CHAT_ID" }` |

`body_env` exists because scripts have no I/O — a script cannot read the
environment. If a value must sit in the body and comes from the environment, it
goes in `body_env`; only the named keys are touched, so user text is never
expanded.

For `run`, `${VAR}` is expanded in `command` and `args` too. A referenced
variable that is not set is an error, never an empty string. Prefer reading
values from the **environment** inside the program (step 6) over passing them in
`args` — anything in `args` is visible in `ps`.

### 5. Say what it does — in one sentence

Give every plugin a `description`. It is optional and the plugin runs without
one, but Admin → Plugins and `openguild plugin list` show it, and that is where
someone decides whether to let your plugin run on their machine:

```toml
name        = "telegram-quest-status"
description = "Sends a Telegram message when a quest tagged `notify` changes status. Needs TELEGRAM_BOT_TOKEN and TELEGRAM_CHAT_ID."
scope       = ["cli", "gui"]
```

Every other field is machine-readable. Say what goes out and where, and name the
values they must supply — those are the two questions they actually have. Max
500 characters; longer notes belong in a README next to `plugin.toml`.

### 6. Ask the user for what only they can supply

A plugin that needs a token, a chat id, or an on/off choice declares it in
`[[inputs]]`. Admin → Plugins renders a widget under the description, stores what
the user picks per guild, and the value arrives three ways at run time.

```toml
[[inputs]]
    key    = "BOT_TOKEN"
    label  = "Bot token"
    secret = true
    help   = "Get one from @BotFather."

[[inputs]]
    key     = "ON_COMMENT"
    label   = "Notify on comments"
    type    = "checkbox"
    default = false
```

**The three ways are not interchangeable:**

- `${KEY}` in `url`, `headers`, `body_env`, `command`, `args` — always a string.
- `config("KEY")` in a script — **keeps its type**, so a checkbox is a real
  boolean and `if config("ON_COMMENT")` works.
- For `run`, an **environment variable of the child** (`$KEY` in the shell
  script) — the natural place for a hook that has no url or header.
  `OPENGUILD_PLUGIN_DIR` / `OPENGUILD_PLUGIN_DATA_DIR` cannot be overridden by a
  value with the same name.

Resolution order is stored value → process environment → `default`. That middle
step is deliberate: a plain environment variable is the "same value in every
guild" layer, and Admin → Plugins overrides it per guild.

Behaviour toggles belong in the script or in `when`, never in a second copy of
the plugin. Do not put a real token in `help` or `default` — the loader rejects
it, and `.guild/plugins/` is committed.

### 7. Write it, then prove it fires

**Do not hand the user an untested plugin.** See "Verifying" below. The two
cheapest checks come first and need no guild at all:

```bash
openguild plugin check <folder>   # what the loader checks, plus the usual mistakes
openguild plugin test  <folder>   # the *.test.rhai functions
```

Write tests for anything with a filter. `fire("quest.created", #{…})` runs the
real path — line matching, `when`, `with` — and records the commands instead of
performing them, so nothing leaves the machine.

### 8. Tell them how to turn it on

```bash
openguild plugin allow <name>        # prints what they would be consenting to
openguild plugin allow <name> --yes  # then actually allows it
```

If the desktop app or a server is **already running**, it still holds the
plugins it loaded at startup — a new or edited definition does nothing there
until it is reloaded: Admin → Plugins → **Reload** in the app, or on the
server's own machine `openguild plugin reload --remote http://<server>`
(refused from other machines). The CLI reads plugins on every run and needs
nothing.

If the plugin declares `inputs`, point them at Admin → Plugins instead of
telling them to export anything. From a terminal the equivalent is:

```bash
echo -n <value> | openguild plugin set <name> <KEY>   # stdin: stays out of history
openguild plugin config <name>                        # what is set, secrets masked
```

Point editors at the schema, too — `openguild plugin schema --out plugin.schema.json`
and a `#:schema` line at the top of `plugin.toml` gives completion and errors in
the editor, built from the events this build actually knows.

## The rules that bite

- **`scope` is required.** Omitting it fails the load.
- **Event names must exist**, and `pre` is only accepted for events that have a
  pre phase. A typo fails the load rather than silently never firing.
- **A line has exactly one "when" and one "what".** `pre` or `post`, never both;
  `call` or `action`, never both.
- **Names must be unique** across the guild's plugin folder **and** every source
  the guild uses. A clash is refused at load — consent is stored by name, so two
  plugins sharing one would share one consent.
- **`description` is part of the consent fingerprint**, like everything else in
  the definition. Editing the wording asks the user again. That is intended —
  the description is what they read when they decided — but do not churn it.
- **Every file in the plugin folder is part of the consent fingerprint.**
  Editing the script — or the `hook.py` a `run` action calls — asks again. Tell
  the user this, or they will think consent is broken. (Imported files outside
  the folder are the exception; the consent screen says so.)
- **A `run` hook must never write into its own folder.** Its working directory
  is a separate per-machine data folder, not the plugin folder, precisely
  because of the rule above: a hook that wrote a log next to itself changed the
  fingerprint and **silently revoked its own consent** after one run (BUG-279).
  Relative paths land in the data folder, which is what you want. To reach a
  file that ships with the plugin, use `${OPENGUILD_PLUGIN_DIR}`:

  ```toml
  args = ["${OPENGUILD_PLUGIN_DIR}/notify.sh"]
  ```

- **`post` lines do not hold the guild up; `pre` lines do.** A `post` line is
  fire-and-forget on its own thread (a CLI command gives delivery ~2s before
  exiting) unless it sets `wait = true`. A `pre` line **always waits** — that is
  what lets it block — so keep it short and give it a `timeout_ms`. Decide what
  a timeout means with `on_timeout` (`continue`, the default, or `block`); the
  same choice exists for a script that throws, as `on_error`.
- **One plugin's failure never affects another**, or the guild.
- **To call `openguild` back, use `$OPENGUILD_GUILD_DIR`.** The working directory
  is deliberately outside the guild (above), so `openguild` cannot find one on its
  own — `openguild --guild "$OPENGUILD_GUILD_DIR" quest comment add …`. This is
  what makes a hook that *answers* (an AI replying to a discussion comment)
  possible at all.
- **A hook that changes the guild does not trigger itself.** If a `run` hook
  calls `openguild …`, the change it makes is not sent back to the same plugin
  (other plugins still get it), so "on tag change, add a tag" does not loop.
  This works because the child inherits `OPENGUILD_PLUGIN_CHAIN` — **do not
  clear the environment** (`env -i`, `env -u OPENGUILD_PLUGIN_CHAIN`) before
  calling `openguild`, and if a `post` target relays into an openguild server,
  forward the `X-OpenGuild-Plugin-Chain` header it received. `e.origin.by` /
  `e.origin.chain` tell a script who caused the event (`"user"` or `"plugin"`).
  Being an environment variable, it follows grandchildren across a pipe, so
  `claude -p | openguild quest comment add …` is safe; detaching the work
  (`nohup … &`) so the guild is changed later by some other path is not.
- **Events are raised by the process that performs the write.** Nothing watches
  files for them. A hook's CLI child that posts a comment raises `comment.added`
  *inside that child*, where the chain above applies; a desktop app holding the
  same guild open raises nothing for it. When a user reports "it works in the
  CLI but not in the app", this is usually not where the difference is — check
  the hook's own receipt first.
- **Handing work to a program is not the same as finishing it.** A `run` hook
  that pipes a comment into `claude -p` and stops there succeeds, exits 0, and
  produces nothing a user can see — the answer goes to a stdout no one reads.
  If the point is a visible result, the same command line has to write it back
  (`… | openguild --guild "$OPENGUILD_GUILD_DIR" quest comment add …`). Say so
  in the input's `help`: a user reading "e.g. `claude -p`" will type exactly
  that and then report the plugin as broken (BUG-335).
- **`send` is for `post` actions and `run` is for `run` actions.** Calling the
  wrong one, or naming an action that does not exist, does nothing at run time —
  the line just goes quiet. `openguild plugin check` catches both.
- **rhai backtick strings do not process escapes.** `` `a\nb` `` sends a literal
  backslash-n (a Telegram message arrived with `\n` in it). Use `"\n"` in a
  double-quoted string and join: `` `${e.quest.id}` + "\n" + e.quest.title ``.
- **A payload can be a plain string.** For `run`, the program gets it as a JSON
  string on stdin (`"/path/to/file"` — strip the quotes). Handy when the program
  only needs one value and should not parse JSON.
- **A shell script does not run on Windows** — there is no `sh`. If the user may
  be on Windows (or you do not know), give the `run` a `windows` variant;
  `macos` and `linux` exist too, and the variant for the machine replaces
  `command`/`args` there (`timeout_ms` is shared):

  ```toml
  [actions.notify.run]
      command = "sh"
      args    = ["${OPENGUILD_PLUGIN_DIR}/hook.sh"]

      [actions.notify.run.windows]
          command = "powershell"
          args    = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                     "-File", "${OPENGUILD_PLUGIN_DIR}/hook.ps1"]
  ```

  This applies to a one-line `sh -c …` just as much as to a script file — that
  is how the one example without a `.sh` got missed (BUG-308). Write the `.ps1`
  in **ASCII** (Windows PowerShell 5.1 reads a BOM-less script in the system
  code page) and read stdin as UTF-8 explicitly —
  `New-Object System.IO.StreamReader([Console]::OpenStandardInput(), (New-Object System.Text.UTF8Encoding($false)))`
  — or non-ASCII paths and titles arrive garbled. A JSON string payload parses
  with `@(ConvertFrom-Json ('[' + $raw + ']'))[0]` (5.1 rejects a bare top-level
  string). Better still, when the job is just "send it somewhere", use `post` —
  it has no OS.
- **Shell hooks: `[ -e "$f" ] && exit 0` under `set -e` kills the script** when
  the file does *not* exist — the list is the last command, not a condition. Use
  `if [ -e "$f" ]; then exit 0; fi`.

## Filtering to specific quests

The common request is "notify me about *this* quest", not all of them. Use a
**tag** — it needs no new concept and the user can turn it off by removing the
tag:

```bash
openguild quest tag add DEV-001 notify
```

```toml
[[handlers]]
    post   = ["quest.status_changed"]
    action = "out"
    [handlers.when]
        "quest.tags" = "notify"
```

No script needed. For a comment event, the tags live on the document the comment
is on, so ask for it: `with = ["subject"]` and `"subject.tags" = "notify"`.

## Verifying (do this before handing it over)

Never point the first test at the real service. Check the definition, run the
script tests, then stand up a local receiver and show the user the exact bytes
their service will get.

```bash
# 0. before anything touches a guild
openguild plugin check <plugin folder>
openguild plugin test  <plugin folder>

# 1. a receiver that prints what it got
python3 - <<'PY' &
import http.server
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get('content-length', 0))
        print("PATH:", self.path); print("BODY:", self.rfile.read(n).decode(), flush=True)
        self.send_response(200); self.send_header('content-length','2'); self.end_headers()
        self.wfile.write(b'{}')
    def log_message(self, *a): pass
http.server.HTTPServer(('127.0.0.1', 8899), H).serve_forever()
PY

# 2. a scratch guild — never the user's real one
export OPENGUILD_HOME=/tmp/pluginlab/home   # consent goes here, not their real file
mkdir -p /tmp/pluginlab && cd /tmp/pluginlab && openguild init --name lab

# 3. the plugin, with the url pointed at the receiver — use it in place, no copy
openguild plugin add <plugin folder>
openguild plugin trust --yes

# 4. trigger it — and check the negative case too
openguild quest new --type DEV --title "테스트"     # should NOT fire if filtered
openguild quest tag add DEV-001 notify
openguild quest move DEV-001 in_progress            # should fire
```

Check four things, not one:

1. It fires when it should.
2. It does **not** fire when it should not — a filter that never rejects is not
   a filter.
3. The body is exactly what the target service expects.
4. For a `pre` line: the change is **actually refused**, the reason reaches the
   user, and whatever the line was supposed to record still got recorded.

For `run`, have the program append to a file and read the file.

## When it does not fire

In this order:

1. `openguild plugin check <folder>` — a broken definition, a missing function,
   a misnamed action.
2. `openguild plugin list` — is it `active`, awaiting consent, or in the error
   list? A broken definition shows its reason there too.
3. Is `scope` right for where you are running? A `gui` plugin never fires from
   the CLI.
4. Did `when` reject it, or did the script return early? Loosen the condition
   temporarily.
5. Is the event name right, and does it have the phase you asked for?
   `openguild plugin events`.
6. Is a long-running app or server still using the old definition? They load
   once at startup — reload (Admin → Plugins → Reload, or
   `openguild plugin reload --remote …` on the server's machine).
7. Did the CLI exit before delivery finished? Raise `OPENGUILD_PLUGIN_DRAIN_MS`,
   or check stderr for the cut-short warning.
8. Delivery failures are reported — stderr on the CLI, startup lines on the
   server, "recent delivery failures" in the desktop app's Plugins tab.

## Reference

- **Field reference: the `openguild` skill's `reference/plugins.md`.** Every
  field, every script call, the event vocabulary. Read it before writing TOML.
- Working examples in `examples/plugins/` of the openguild repo — the folder
  itself works as a source (`openguild plugin source add <repo>/examples/plugins`):
  - `telegram-quest-status` — post + script + tag filter + `inputs`, with tests
  - `discussion-to-ai` — run, hands the comment to something on this machine
    (inbox file / command / tmux pane); `select` inputs pick which, with tests
  - `desktop-notify` — run, no script at all
  - `deleted-audit` — two `pre` lines, one of which **blocks**
  - `backup-archive` — run on `backup.created`, a plain-string payload,
    `inputs` read as environment variables
  - `examples/plugins/README.md` — read this before writing one from scratch
- `openguild plugin events` for event names, phases and `with` names;
  `openguild plugin --help` for commands.
