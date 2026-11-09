#!/usr/bin/env python3
"""Optional Linux menu entry; run after extracting the portable directory."""
import os
from pathlib import Path

if __name__ == '__main__':
    base = Path(__file__).resolve().parent
    if not (base / 'HashHelm').is_file(): raise SystemExit('Run the copy beside the packaged HashHelm executable.')
    data = Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share')))
    if not data.is_absolute(): raise SystemExit('XDG_DATA_HOME must be absolute.')
    desktop = data / 'applications/hashhelm.desktop'
    desktop.parent.mkdir(parents=True, exist_ok=True)
    # Desktop Entry spec: escape reserved characters in a quoted Exec argument.
    command = str(base / 'launch.sh').replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%')
    desktop.write_text(f'[Desktop Entry]\nType=Application\nName=HashHelm\nComment=Local Hashcat workbench\nExec="{command}"\nIcon={base / "assets/branding/digiforensics.png"}\nTerminal=false\nCategories=Utility;\n', encoding='utf-8')
    print(f'Created {desktop}. Run this script again if you move the portable folder.')
