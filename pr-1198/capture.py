import argparse
from pathlib import Path
import os
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('version', choices=['before', 'after'])
parser.add_argument('--only', help='capture just this scene key')
parser.add_argument('--skip-existing', action='store_true')
args = parser.parse_args()
root = Path(__file__).resolve().parent
captures = root / 'site' / 'pr-1198'
captures.mkdir(parents=True, exist_ok=True)
binary = root / 'bin' / args.version
(root / 'art').mkdir(exist_ok=True)

def xdo(*words):
    return subprocess.check_output(['xdotool', *map(str, words)], text=True).strip()

for view, page in [('episodes', 'episodes'), ('podcast', 'show:sh0')]:
    for theme in ['dark', 'light']:
        for width, height in [(720, 600), (1280, 800)]:
            for state in ['top', 'end', 'menu']:
                scene = f'{view}-{theme}-{width}-{state}'
                if args.only and scene != args.only:
                    continue
                profile = root / 'profiles' / f'{args.version}-{scene}'
                art = profile / 'cache' / 'art'
                art.parent.mkdir(parents=True, exist_ok=True)
                if not art.exists():
                    art.symlink_to(root / 'art', target_is_directory=True)
                target = captures / f'{args.version}-{scene}.png'
                if args.skip_existing and target.exists():
                    continue
                command = [str(binary), '--demo', '--demo-data', str(profile),
                           '--demo-page', page, '--demo-show', theme,
                           '--demo-size', f'{width}x{height}', '--demo-shot', str(target),
                           '--demo-shot-delay', '4000']
                with (root / 'logs' / f'{args.version}-{scene}.log').open('w') as log:
                    process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
                    try:
                        deadline = time.monotonic() + 20
                        window = None
                        while time.monotonic() < deadline:
                            if process.poll() is not None:
                                raise RuntimeError(f'{scene}: app exited with {process.returncode}')
                            try:
                                found = xdo('search', '--onlyvisible', '--name', '^Spotifast$')
                                if found:
                                    window = found.splitlines()[-1]
                                    break
                            except subprocess.CalledProcessError:
                                pass
                            time.sleep(0.05)
                        if window is None:
                            raise RuntimeError(f'{scene}: no visible window')
                        xdo('windowfocus', '--sync', window)
                        xdo('mousemove', '1590', '990')
                        time.sleep(0.65)
                        if state != 'top':
                            xdo('key', '--clearmodifiers', 'End')
                            time.sleep(0.35)
                        if state == 'menu':
                            xdo('mousemove', '--window', window, int(width * 0.75), int(height * 0.5))
                            xdo('click', '3')
                            time.sleep(0.2)
                            xdo('mousemove', '1590', '990')
                        process.wait(timeout=30)
                        if process.returncode or not target.exists():
                            raise RuntimeError(f'{scene}: screenshot failed; see {log.name}')
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            process.wait(timeout=5)
                print(target.name, flush=True)
