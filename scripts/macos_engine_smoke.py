"""Verify the app seal while Hashcat runs and after it is forcibly stopped."""
from pathlib import Path
import os
import selectors
import signal
import subprocess
import tempfile


def check(app):
    app = Path(app).resolve(strict=True)
    engine = app / 'Contents/Resources/hashcat/hashcat'
    verify = ['codesign', '--verify', '--deep', '--strict', str(app)]
    subprocess.run(verify, check=True)
    with tempfile.TemporaryDirectory(prefix='hashhelm-pid-check-') as temp:
        cache = Path(temp)
        with (cache / 'stderr.log').open('w+') as errors:
            child = subprocess.Popen(
                [str(engine), '--cache-path', str(cache), '--session', 'hashhelm-pid-check',
                 '--stdout', '-a', '3', '?d' * 12, '--quiet'],
                cwd=cache, stdout=subprocess.PIPE, stderr=errors, stdin=subprocess.DEVNULL,
                start_new_session=True,
            )
            try:
                with selectors.DefaultSelector() as selector:
                    selector.register(child.stdout, selectors.EVENT_READ)
                    assert selector.select(timeout=120), 'Hashcat did not produce fixture candidates'
                    assert os.read(child.stdout.fileno(), 256), 'Hashcat exited without fixture output'
                assert child.poll() is None, 'Fixture must still be running during seal validation'
                assert (cache / 'hashhelm-pid-check.pid').is_file(), 'PID lock was not placed in the writable cache'
                subprocess.run(verify, check=True)
            finally:
                if child.poll() is None:
                    os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=10)
                child.stdout.close()
            subprocess.run(verify, check=True)
    print('macOS app seal passed during engine execution and after forced termination.', flush=True)


if __name__ == '__main__':
    import sys
    check(sys.argv[1])
