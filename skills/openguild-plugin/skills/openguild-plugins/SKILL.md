---
name: openguild-plugins
description: Write an openguild plugin — an event hook that runs the user's own program when something happens in their guild (a quest is created, a status changes, a comment is added). Use when the user asks to get notified about guild activity (Telegram, Slack, Discord, email, a webhook, a desktop notification), to run a script or command on a guild event, to send quest/comment content to an AI or another service, or mentions `.guild/plugins`, `plugin.json`, `openguild plugin`, or "openguild hook". Also use to debug a plugin that is not firing.
---

# Writing an openguild plugin

A plugin calls **the user's own program** when something happens in their guild.
It is three files at most:

```
.guild/plugins/{name}/plugin.json    what/where/how          (committed to git)
.guild/plugins/{name}/*.rhai         whether/what shape      (optional, committed)
.guild/plugins/{name}/anything-else  the program `run` calls (optional, committed)
```

Consent is **not** committed — it lives in `~/.openguild/plugin-consent.json` on
each machine. A plugin arriving by `git pull` does not run until someone here
allows it.

## Work in this order

Skipping to the JSON is the usual way to get this wrong. Each step below rules
out a class of plugin that cannot work.

### 1. What should happen, and where?

Ask, or infer from what they said:

- **Which event.** Run `openguild plugin events` for the list. If nothing fits,
  say so — inventing a name silently produces a plugin that never fires.
- **Which machine.** `scope` is required and has no default:
  - `cli` — their terminal. Fires on `openguild quest move …`, batch scripts, CI.
  - `gui` — the desktop app only. Right for anything that interrupts a person
    (a desktop notification on a CLI batch of 20 quests fires 20 times).
  - `server` — applies to **everyone using that server**, not just them.
- **`post` or `run`.** Those two are all the core owns. Slack, Discord, email,
  Telegram, an AI call — all of them are `post` to that service's URL, or `run`
  of a program the user writes. Do not look for a Slack action; there isn't one.

### 2. Does it need a filter or a different shape?

Only if the answer is yes do you write a `.rhai` script.

- **No script** → the plugin fires on every subscribed event and sends the whole
  event JSON. Fine for `run`, where the program can filter in whatever language
  it is already written in.
- **Script** → `should_send(event)` decides, `payload(event)` reshapes. Both
  optional. Required when the target has a fixed body shape (Telegram wants
  `{chat_id, text}`) or when only some events should go out.

### 3. Where do the secrets go?

`plugin.json` is committed. A literal API key in it is in the history forever,
so the loader **rejects** anything that looks like one.

| Where the secret goes | How |
|---|---|
| In the URL | `"url": "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage"` |
| In a header | `"headers": { "Authorization": "Bearer ${MY_KEY}" }` |
| In the **body** | `"body_env": { "chat_id": "TELEGRAM_CHAT_ID" }` |

`body_env` exists because scripts have **no I/O at all** — `payload()` cannot
read the environment. That is the point of the sandbox, not a gap to work
around. If a value must be in the body and comes from the environment, it goes
in `body_env`.

For `run`, `${VAR}` is expanded in `command` and `args` too. A referenced
variable that is not set is an error, never an empty string.

### 4. Write it, then prove it fires

**Do not hand the user an untested plugin.** See "Verifying" below.

### 5. Tell them how to turn it on

```bash
openguild plugin allow <name>        # prints what they would be consenting to
openguild plugin allow <name> --yes  # then actually allows it
```

Say which environment variables they must export, and where to get each one.

## The rules that bite

- **`scope` is required.** Omitting it fails the load.
- **`on` names must exist.** A typo fails the load rather than silently never
  firing. `pre:` is only accepted for events that actually emit a pre phase.
- **Names must be unique** across the guild's plugin folders.
- **Every file in the plugin folder is part of the consent fingerprint.**
  Editing the script — or the `hook.py` a `run` action calls — asks again. Tell
  the user this, or they will think consent is broken.
- **The hook never blocks the guild.** Delivery is fire-and-forget on its own
  thread; a CLI command gives it ~2s before exiting. A plugin that must finish
  should be `run` with a short program, not a slow HTTP call from the CLI.
- **One plugin's failure never affects another**, or the guild.

## Filtering to specific quests

The common request is "notify me about *this* quest", not all of them. Use a
**tag** — it needs no new concept and the user can turn it off by removing the
tag:

```bash
openguild quest tag add DEV-001 notify
```

```rhai
fn should_send(e) { e.ok && e.quest.tags.contains("notify") }
```

## Verifying (do this before handing it over)

Never point the first test at the real service. Stand up a local receiver, aim
the plugin at it, and show the user the exact bytes their service will get.

```bash
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

# 3. the plugin, with the url pointed at the receiver
cp -R <plugin folder> .guild/plugins/
openguild plugin trust --yes

# 4. trigger it — and check the negative case too
openguild quest new --type DEV --title "테스트"     # should NOT fire if filtered
openguild quest tag add DEV-001 notify
openguild quest move DEV-001 in_progress            # should fire
```

Check three things, not one:

1. It fires when it should.
2. It does **not** fire when it should not — a filter that never rejects is not
   a filter.
3. The body is exactly what the target service expects.

For `run`, have the program append to a file and read the file.

## When it does not fire

In this order:

1. `openguild plugin list` — is it `active`, awaiting consent, or in the error
   list? A broken definition shows its reason there.
2. Is the guild's `scope` right for where you are running? A `gui` plugin never
   fires from the CLI.
3. Did the script's `should_send` return false? Make it `true` temporarily.
4. Is the event name right? `openguild plugin events`.
5. Did the CLI exit before delivery finished? Raise
   `OPENGUILD_PLUGIN_DRAIN_MS`, or check stderr for the cut-short warning.
6. Delivery failures are reported — stderr on the CLI, startup lines on the
   server, "recent delivery failures" in the desktop app's Plugins tab.

## Reference

- Working examples to copy: `examples/plugins/` in the openguild repo —
  `telegram-quest-status` (post + script + tag filter), `desktop-notify`
  (run, no script), `deleted-audit` (run + observational `pre`).
- Full field reference: the `openguild` skill's `reference/plugins.md`.
- `openguild plugin events` for event names, `openguild plugin --help` for
  commands.
