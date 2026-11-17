#!/usr/bin/env python3
"""Checks the bundled translations for drift.

Every screen string is marked with @tr("...") in app/ui/*.slint and translated in
app/translations/<locale>/LC_MESSAGES/omacontrol.po.

`cargo test -p omacontrol` already fails when a locale is missing a screen string or leaves
it empty (`every_screen_message_is_translated_in_each_bundled_locale`). This script catches
what that test cannot:

  error    a translation drops or adds a `{}` placeholder (the screen would show a garbled
           line, or the argument would not fit)
  error    a duplicate msgid (gettext silently picks one of them)
  warning  a message no screen uses any more (a leftover in every catalog)
  warning  catalogs that disagree with each other

Usage: python3 scripts/check_translations.py [--verbose]
(locales live in app/translations; pass --all to print every unused message)
Exits 1 when a catalog needs fixing, 0 otherwise.
"""
import glob
import os
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
UI_DIR = os.path.join(ROOT, "app", "ui")
PO_GLOB = os.path.join(ROOT, "app", "translations", "*", "LC_MESSAGES", "omacontrol.po")

UNESCAPE = {"\\": "\\", '"': '"', "n": "\n", "t": "\t", "r": "\r"}


def unescape(text):
    """Unescapes the `\\"` and `\\n` gettext and Slint strings both use."""
    out = []
    escaped = False
    for character in text:
        if escaped:
            out.append(UNESCAPE.get(character, character))
            escaped = False
        elif character == "\\":
            escaped = True
        else:
            out.append(character)
    return "".join(out)


def quoted_content(source, quote_at):
    """The content of the string that opens at `quote_at`, and the index just past it."""
    start = quote_at + 1
    escaped = False
    for offset, character in enumerate(source[start:], start):
        if character == '"' and not escaped:
            return unescape(source[start:offset]), offset + 1
        escaped = character == "\\" and not escaped
    return None, len(source)


def screen_messages():
    """Every @tr("...") message of the screens, with the file that uses each one."""
    messages = {}
    for path in sorted(glob.glob(os.path.join(UI_DIR, "*.slint"))):
        with open(path, encoding="utf-8") as file:
            source = file.read()
        cursor = 0
        while True:
            found = source.find('@tr("', cursor)
            if found < 0:
                break
            message, cursor = quoted_content(source, found + 4)
            if message is None:
                break
            messages.setdefault(message, os.path.basename(path))
    return messages


def read_catalog(path):
    """The catalog as {msgid: msgstr}, its duplicates, and the gettext header."""
    catalog = {}
    duplicates = []
    header = {}
    msgid = msgstr = None
    field = None

    def store():
        if msgid is None:
            return
        if msgid == "":
            # The header: "Project-Id-Version: OmaControl 0.3.6\n..."
            for line in msgstr.splitlines():
                key, _, value = line.partition(":")
                header[key.strip()] = value.strip()
            return
        if msgid in catalog:
            duplicates.append(msgid)
        catalog[msgid] = msgstr

    with open(path, encoding="utf-8") as file:
        lines = file.read().splitlines()
    for line in lines:
        if line.startswith("msgid "):
            store()
            msgid, _ = quoted_content(line, len("msgid "))
            msgstr, field = "", "msgstr"
        elif line.startswith("msgstr "):
            msgstr, _ = quoted_content(line, len("msgstr "))
            field = "msgstr"
        elif line.startswith('"') and field == "msgstr":
            # A continuation line: gettext splits long strings across several lines.
            more, _ = quoted_content(line, 0)
            msgstr += more or ""
    store()
    return catalog, duplicates, header


def placeholders(message):
    return message.count("{}")


def main(argv):
    verbose = "--verbose" in argv or "--all" in argv
    screens = screen_messages()
    paths = sorted(glob.glob(PO_GLOB))
    problems = 0
    warnings = 0
    unused = set()

    if not paths:
        print(f"no catalogs under {os.path.relpath(os.path.join(ROOT, 'app', 'translations'))}")
        return 1

    for path in paths:
        locale = path.split(os.sep)[-3]
        catalog, duplicates, header = read_catalog(path)
        issues = []

        for msgid in sorted(duplicates):
            issues.append(("error", f'duplicate msgid "{msgid}"'))

        for msgid, msgstr in catalog.items():
            if msgid not in screens:
                unused.add(msgid)
                continue
            if not msgstr.strip():
                issues.append(("error", f'"{msgid}": empty translation'))
            elif placeholders(msgid) != placeholders(msgstr):
                issues.append(
                    (
                        "error",
                        f'"{msgid}": {placeholders(msgstr)} placeholder(s) in the '
                        f"translation, {placeholders(msgid)} in the message",
                    )
                )

        # The version the catalogs were last touched for, so a release can update them.
        version = header.get("Project-Id-Version", "")
        expected = f"OmaControl {workspace_version()}"
        if version and version != expected:
            issues.append(("warning", f"header says {version!r}, the workspace is {expected!r}"))

        problems += sum(1 for level, _ in issues if level == "error")
        warnings += sum(1 for level, _ in issues if level == "warning")

        if issues:
            print(f"{os.path.relpath(path, ROOT)}")
            for level, text in issues:
                print(f"  {level}: {text}")

    if unused:
        print(f"\n{len(unused)} message(s) no screen uses:")
        for msgid in sorted(unused):
            if verbose or msgid == sorted(unused)[0]:
                print(f'  "{msgid}"')
        if not verbose and len(unused) > 1:
            print("  (run with --verbose to list them all)")

    print(
        f"\n{len(paths)} locale(s), {len(screens)} screen message(s): "
        f"{problems} problem(s), {warnings} warning(s)"
    )
    return 1 if problems else 0


def workspace_version():
    """The version in Cargo.toml's [workspace.package], without a TOML parser."""
    with open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8") as file:
        section = False
        for line in file:
            line = line.strip()
            if line.startswith("["):
                section = line == "[workspace.package]"
            elif section and line.startswith("version"):
                return line.split("=", 1)[1].strip().strip('"')
    return "unknown"


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
