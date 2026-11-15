#!/usr/bin/env python3
"""Export .docx files to PDF with the local Microsoft Word, caching by content hash.

    word_export.py OUT_DIR FILE.docx [FILE.docx ...]

Word for Mac is sandboxed and its "Grant File Access" dialog grants single files, never a
folder. So every document is staged under the same two names, STAGE/t.docx and STAGE/t.pdf
(STAGE defaults to ~/Documents/word-parity; override with WORD_PARITY_STAGE), which need
granting once; after that no export prompts. OUT_DIR/<sha256>.pdf is
reused when present, and OUT_DIR/<sha256>.json records the Word build that made it.
"""
import fcntl
import hashlib
import json
import os
import shutil
import subprocess
import sys
import threading
import time

WORD = "/Applications/Microsoft Word.app"
EXPORT_TIMEOUT = int(os.environ.get("WORD_PARITY_EXPORT_TIMEOUT", "60"))
LOCK = os.path.expanduser(os.environ.get("WORD_PARITY_LOCK", "~/word-parity-data/word.lock"))
STAGE = os.path.expanduser(os.environ.get("WORD_PARITY_STAGE", "~/Documents/word-parity"))


# Word shows a few modal alerts ("the file may be damaged") that no scripting setting
# suppresses and that block every later export. While an export runs, a watchdog presses
# the confirm-only button of such an alert. Dialogs offering a choice (update fields?
# recover?) are left alone, except "open read-only", which an export never minds: answering them could change the layout being measured.
DISMISS = ["确定", "OK", "好"]
DISMISS_SCRIPT = """
tell application "System Events"
  if not (exists process "Microsoft Word") then return ""
  tell process "Microsoft Word"
    set pressed to ""
    repeat with w in windows
      set names to name of every button of w
      if (count of names) is 1 and ((item 1 of names) is in {%s}) then
        click button 1 of w
        set pressed to pressed & (item 1 of names)
      else if names contains "只读" then
        click button "只读" of w
        set pressed to pressed & "只读"
      else if names contains "Read Only" then
        click button "Read Only" of w
        set pressed to pressed & "Read Only"
      end if
    end repeat
    return pressed
  end tell
end tell
""" % ", ".join('"%s"' % n for n in DISMISS)


class Watchdog(threading.Thread):
    def __init__(self):
        super().__init__(daemon=True)
        self.stop = threading.Event()
        self.pressed = 0

    def run(self):
        while not self.stop.wait(2):
            out = subprocess.run(["osascript", "-e", DISMISS_SCRIPT], capture_output=True, text=True)
            if out.stdout.strip():
                self.pressed += 1


def close_staged(name: str) -> None:
    """Close a staged document a failed export left open, so the next one is not locked."""
    script = f'''
    tell application "Microsoft Word"
      repeat 10 times
        if not (exists document "{name}") then exit repeat
        try
          close document "{name}" saving no
        on error
          delay 1
        end try
      end repeat
    end tell
    '''
    subprocess.run(["osascript", "-e", script], capture_output=True, text=True, timeout=60)


def word_build() -> str:
    out = subprocess.run(["defaults", "read", f"{WORD}/Contents/Info", "CFBundleVersion"],
                         capture_output=True, text=True, check=True)
    return out.stdout.strip()


def export(src: str, pdf: str) -> None:
    # Every document is staged as t.docx, so a previous one must be fully closed first and
    # the new one fetched by name: `open` returns nothing when a same-named file is open.
    name = os.path.basename(src)
    script = f'''
    with timeout of 600 seconds
      tell application "Microsoft Word"
        repeat 30 times
          if not (exists document "{name}") then exit repeat
          try
            close document "{name}" saving no
          on error
            delay 1
          end try
        end repeat
        open file name (POSIX file "{src}" as text) without add to recent files
        set d to missing value
        repeat 60 times
          try
            set d to document "{name}"
            get name of d
            exit repeat
          on error
            delay 0.5
          end try
        end repeat
        save as d file name (POSIX file "{pdf}" as text) file format format PDF
        repeat 30 times
          try
            close d saving no
            exit repeat
          on error
            delay 1
          end try
        end repeat
      end tell
    end timeout
    '''
    for attempt in range(3):
        try:
            subprocess.run(["osascript", "-e", script], check=True, capture_output=True, text=True,
                           timeout=EXPORT_TIMEOUT)
            return
        except subprocess.TimeoutExpired:
            # a modal dialog (password, field update, repair) holds Word: give up on this file
            raise subprocess.CalledProcessError(1, "osascript", stderr="timed out (Word showed a dialog?)")
        except subprocess.CalledProcessError:
            if attempt == 2:
                raise
            time.sleep(2)


def main() -> None:
    out_dir, files = sys.argv[1], sys.argv[2:]
    os.makedirs(out_dir, exist_ok=True)
    build = word_build()
    for path in files:
        data = open(path, "rb").read()
        digest = hashlib.sha256(data).hexdigest()
        pdf = os.path.join(out_dir, digest + ".pdf")
        meta = os.path.join(out_dir, digest + ".json")
        if os.path.exists(pdf) and os.path.exists(meta) and json.load(open(meta)).get("word") == build:
            print(f"cached {digest[:12]} {path}")
            continue
        # One Word, one staging name: sessions running exports in parallel take turns.
        lock = open(LOCK, "w")
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            export_one(path, pdf, meta, digest, build)
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)
            lock.close()


def export_one(path, pdf, meta, digest, build):
    staged = os.path.join(STAGE, "t.docx")
    staged_pdf = os.path.join(STAGE, "t.pdf")
    if os.path.exists(staged_pdf):
        os.remove(staged_pdf)
    stale = os.path.join(STAGE, "~$" + os.path.basename(staged))
    if os.path.exists(stale):
        os.remove(stale)
    shutil.copyfile(path, staged)
    start = time.time()
    dog = Watchdog()
    dog.start()
    try:
        export(staged, staged_pdf)
    except subprocess.CalledProcessError as e:
        print(f"FAILED {path}: {e.stderr.strip()}", file=sys.stderr)
        close_staged(os.path.basename(staged))
        return
    finally:
        dog.stop.set()
    if dog.pressed:
        print(f"note {path}: dismissed {dog.pressed} Word alert(s)", file=sys.stderr)
    shutil.copyfile(staged_pdf, pdf)
    json.dump({"source": os.path.abspath(path), "sha256": digest, "word": build,
               "seconds": round(time.time() - start, 2),
               "alertsDismissed": dog.pressed}, open(meta, "w"), indent=1)
    print(f"exported {digest[:12]} {path}")


if __name__ == "__main__":
    main()
