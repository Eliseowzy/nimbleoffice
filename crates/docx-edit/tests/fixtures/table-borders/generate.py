"""Rebuild these fixtures from the Word-parity table-border experiment."""
import runpy
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCE = HERE.parents[4] / "tools/word-parity/experiments/table_borders/generate.py"
experiment = runpy.run_path(str(SOURCE))
for name, kw in experiment["cases"]():
    experiment["write"](Path(sys.argv[1]) if len(sys.argv) > 1 else HERE, name, **kw)
