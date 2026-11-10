"""Keep portable Hashcat PID locks out of an immutable macOS app bundle.

The pinned upstream engine already accepts --cache-path. Use that existing,
per-user directory for its session lock too, including after a forced exit.
No hash modes, cryptographic code or command-line options are changed.
"""
from pathlib import Path
import sys


def patch(source):
    path = Path(source) / 'src/pidfile.c'
    original = 'hc_asprintf (&pidfile_ctx->filename, "%s/%s.pid", folder_config->session_dir, user_options->session);'
    replacement = 'hc_asprintf (&pidfile_ctx->filename, "%s/%s.pid", folder_config->cache_dir, user_options->session);'
    text = path.read_text(encoding='utf-8')
    if replacement in text and original not in text:
        return
    if text.count(original) != 1 or replacement in text:
        raise ValueError('Hashcat PID source differs from the reviewed 8.0.0 code; refusing to overwrite it')
    path.write_text(text.replace(original, replacement), encoding='utf-8')


if __name__ == '__main__':
    patch(sys.argv[1])
