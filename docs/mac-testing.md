# DevHub — macOS test setup & verification

**Goal:** build and run Creatio DevHub from source on a Mac, then confirm a few
things work. This is the first Mac to run the app, so the checks matter — report
anything that looks off.

> You'll run it in **dev mode** (from source), so there is **no Gatekeeper /
> "unidentified developer" warning** — that only applies to the packaged `.dmg`.
> Ignore that part for now.

## 1. Install the prerequisites

Open **Terminal** and install these one at a time.

Xcode command-line tools (the C toolchain Rust needs):

```bash
xcode-select --install
```

Homebrew (skip if `brew --version` already works):

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Node 22, GitHub CLI, and .NET (clio runs on .NET):

```bash
brew install node@22 gh dotnet
```

Rust (accept the default install):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then restart Terminal (or `source ~/.zshrc`) so `cargo` and `node` are on PATH.

## 2. Install clio and put it on your PATH

```bash
dotnet tool install -g clio
```

clio installs to `~/.dotnet/tools`. Add that to your PATH permanently:

```bash
echo 'export PATH="$PATH:$HOME/.dotnet/tools"' >> ~/.zshrc && source ~/.zshrc
```

Confirm all four tools resolve:

```bash
clio --version && git --version && gh --version && node --version
```

## 3. Register at least one Creatio environment in clio

**This is important.** One of the specific things we're checking is whether
DevHub can *see* your clio environments on macOS — it couldn't in the first
version; that's now fixed and we need to confirm the fix. So you need at least
one registered environment.

Register one (ask for a test environment's URL + login if you don't have one):

```bash
clio reg-web-app my-test-env -u https://YOUR-ENV.creatio.com -l LOGIN -p PASSWORD
```

Verify clio itself lists it:

```bash
clio show-web-app-list
```

## 4. Get the code

```bash
git clone https://github.com/SmitSuthar8834/creatio-devhub.git
```

```bash
cd creatio-devhub
```

## 5. Run the app

```bash
./dev.sh
```

(If it says permission denied, run `bash dev.sh`.) The **first run compiles all
the Rust dependencies — expect 5–15 minutes.** After that the DevHub window opens
on its own. Later runs are fast.

## 6. Verify these five things

Go through each and note what you see (a screenshot of each is ideal).

1. **App launches** — the DevHub window opens and the sidebar (Environments,
   Workspaces, Packages, etc.) renders with no blank or broken areas.

2. **⭐ Environment discovery (the key one)** — open the **Environments** screen.
   The environment(s) you registered in step 3 should appear, matching
   `clio show-web-app-list`. If DevHub shows **none** but clio lists them, that is
   the bug — flag it immediately.

3. **Tool discovery** — go to **Settings → Command-line tools**. clio, git, and
   gh should each show a resolved path (expect Homebrew / `~/.dotnet/tools`
   locations like `/opt/homebrew/bin/git`, `~/.dotnet/tools/clio`). None should
   say "not found."

4. **A real operation works** — with an environment selected, do something
   read-only, e.g. **ping** it, or open **Packages** and let it list. It should
   complete and show results, not hang or error.

5. **Job cancellation + no orphans** — start a cancellable job (a ping, or a
   package pull) and hit **Cancel**. It should stop promptly. Then, in a separate
   Terminal, confirm nothing is left running:

   ```bash
   ps aux | grep -i clio | grep -v grep
   ```

   That should print **nothing** (an empty result = pass). If a `clio` / `dotnet`
   process is still there minutes later, flag it.

## 7. Report back

Send back:

- Screenshots of the Environments screen (check 2) and Settings → Command-line
  tools (check 3).
- Whether each of the five checks passed.
- Any red error text, a blank screen, or a console error from the Terminal where
  `./dev.sh` is running.

---

Once all five pass, the app is cleared for a macOS release: bump the version, tag
`v*`, and `.github/workflows/release.yml` publishes the universal `.dmg`. See the
"macOS support" section of [`HANDOFF.md`](../HANDOFF.md) for the release gate and
the notarization to-do.
