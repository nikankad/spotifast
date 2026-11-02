import base64
import html
import json
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parent
site = root / 'site'
page = site / 'pr-1198'
images = {}
comparisons = {}
for view in ['episodes', 'podcast']:
    for theme in ['dark', 'light']:
        for width, height in [(720, 600), (1280, 800)]:
            for state in ['top', 'end', 'menu']:
                scene = f'{view}-{theme}-{width}-{state}'
                for version in ['before', 'after']:
                    key = f'{version}-{scene}'
                    image = page / f'{key}.png'
                    dimensions = subprocess.check_output(
                        ['identify', '-format', '%wx%h', str(image)], text=True)
                    assert dimensions == f'{width}x{height}', (key, dimensions)
                    images[key] = 'data:image/png;base64,' + base64.b64encode(image.read_bytes()).decode()
                diff = subprocess.run(['compare', '-metric', 'AE',
                                       str(page / f'before-{scene}.png'),
                                       str(page / f'after-{scene}.png'), 'null:'],
                                      text=True, capture_output=True)
                assert diff.returncode in [0, 1], diff.stderr
                comparisons[scene] = int(float(diff.stderr.strip()))

metadata = {
    'pull_request': 'https://github.com/crmne/spotifast/pull/1198',
    'baseline': '3767d53a0b3f720b00c29eebfa69806020267f8d',
    'candidate': 'c1844fdf5b55b735ff684dd2daf56736a1287b74',
    'platform': 'Linux x86_64, X11 through Xvfb, Mesa software OpenGL',
    'fixture': 'Identical paused demo playback; 1,000 loaded episodes per list; cached placeholder artwork.',
    'different_pixels': comparisons,
}
(page / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
for name in ['fixture.patch', 'capture.py', 'build_site.py']:
    shutil.copy2(root / name, page / name)
identical = sum(value == 0 for value in comparisons.values())
intro = f'{identical} of {len(comparisons)} matching capture pairs are pixel-identical.'
document = '''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Spotifast PR #1198: visual comparison</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#17191d;color:#eef0f4;font:16px/1.5 system-ui,sans-serif}
main{max-width:1600px;margin:auto;padding:24px}h1{font-size:26px;margin:0 0 10px}p{max-width:1050px}
a{color:#b8d5ff}code{font-size:.95em}label{display:grid;gap:5px;font-size:14px}
.controls{display:flex;flex-wrap:wrap;gap:12px;align-items:end;padding:16px 0}
select,button{font:inherit;color:inherit;background:#272b33;border:1px solid #59606d;border-radius:6px;padding:9px 12px}
button{cursor:pointer}button[aria-pressed=true]{background:#d8e6ff;color:#15243c;border-color:#d8e6ff}
.modes{display:flex;gap:8px;flex-wrap:wrap}.grid{display:grid;grid-template-columns:1fr 1fr;gap:16px;align-items:start}
figure{margin:0;min-width:0}figcaption{padding:8px 0;font-weight:600}img{display:block;width:100%;height:auto;border:1px solid #59606d}
#summary{color:#c6d1e3;font-variant-numeric:tabular-nums}.note{font-size:14px;color:#c6cbd5}details{margin:22px 0}
@media(max-width:700px){main{padding:12px}.grid{grid-template-columns:1fr}}
</style>
<main><h1>Spotifast PR #1198</h1>
<p>Compare the same episode views before and after the performance changes. No layout or interaction change is intended.
<a href="https://github.com/crmne/spotifast/pull/1198">Open the pull request</a>.</p>
<p class="note">Linux x86_64, X11 through Xvfb, Mesa software OpenGL, 100% zoom. Both versions use the same shared fixture:
1,000 loaded episodes per list, paused demo playback and cached placeholder artwork. These are actual application framebuffer captures.</p>
<div class="controls">
<label>View<select id="view"><option value="episodes">Saved episodes</option><option value="podcast">Podcast</option></select></label>
<label>Theme<select id="theme"><option value="dark">Dark</option><option value="light">Light</option></select></label>
<label>Window<select id="size"><option value="1280">1280 × 800</option><option value="720">720 × 600</option></select></label>
<label>State<select id="state"><option value="top">Top of list</option><option value="end">End of list</option><option value="menu">Episode menu at end</option></select></label>
<div class="modes" aria-label="Comparison mode"><button data-mode="both" aria-pressed="true">Side by side</button><button data-mode="before" aria-pressed="false">Before</button><button data-mode="after" aria-pressed="false">After</button></div>
</div>
<p id="summary" aria-live="polite"></p>
<div class="grid" id="grid"><figure id="before"><figcaption>Before · <a href="https://github.com/crmne/spotifast/commit/3767d53a0b3f720b00c29eebfa69806020267f8d">3767d53</a> + shared fixture</figcaption><img id="before-image" alt="Before"></figure>
<figure id="after"><figcaption>After · <a href="https://github.com/nikankad/spotifast/commit/c1844fdf5b55b735ff684dd2daf56736a1287b74">c1844fd</a> + shared fixture</figcaption><img id="after-image" alt="After"></figure></div>
<details><summary>Evidence and reproduction</summary><p>__INTRO__
The application was built with its demo feature and MilkDrop disabled in both versions. Only the shared fixture patch was added to each source revision.
The top state leaves the list at its initial position; the end state presses End; the menu state then right-clicks an episode.
Loading, empty, and error-state rendering is unchanged by this patch.</p>
<p><a href="comparison.html" download>Self-contained HTML</a> · <a href="fixture.patch">Shared fixture patch</a> · <a href="capture.py">Capture script</a> · <a href="build_site.py">Comparison generator</a> · <a href="metadata.json">Pixel comparison results</a></p>
<p>Apply the shared fixture patch to each source revision, then build with
<code>cargo build --locked --no-default-features --features demo --bin spotifast</code>.
Copy the resulting executables to <code>bin/before</code> and <code>bin/after</code> next to the capture script,
then run the script under Xvfb with <code>before</code> or <code>after</code> as its argument. The local builds disabled Spotifast debug symbols to fit worker memory;
optimization and layout settings were identical. The paused fixture is temporary QA data and is absent from the PR's source changes.</p></details>
</main><script>
const images=__IMAGES__;
const differences=__DIFFERENCES__;
const $=id=>document.getElementById(id);
let mode='both';
function update(){
 const scene=[$('view').value,$('theme').value,$('size').value,$('state').value].join('-');
 for(const version of ['before','after']){
  $(version+'-image').src=images[version+'-'+scene];
  $(version+'-image').alt=version+' '+scene;
  $(version).hidden=mode!=='both'&&mode!==version;
 }
 $('grid').style.gridTemplateColumns=mode==='both'?'':'1fr';
 const n=differences[scene];
 $('summary').textContent=n===0?'Selected pair: pixel-identical.':`Selected pair: ${n.toLocaleString()} pixels differ; compare the actual captures above.`;
 document.querySelectorAll('[data-mode]').forEach(button=>button.setAttribute('aria-pressed',String(button.dataset.mode===mode)));
}
document.querySelectorAll('select').forEach(select=>select.addEventListener('change',update));
document.querySelectorAll('[data-mode]').forEach(button=>button.addEventListener('click',()=>{mode=button.dataset.mode;update()}));
update();
</script></html>
'''
document = document.replace('__INTRO__', html.escape(intro)).replace('__IMAGES__', json.dumps(images)).replace('__DIFFERENCES__', json.dumps(comparisons))
(page / 'comparison.html').write_text(document)
live = document.replace(json.dumps(images), json.dumps({key: f'{key}.png' for key in images}))
(page / 'index.html').write_text(live)
(site / '.nojekyll').touch()
(site / 'index.html').write_text('<!doctype html><meta charset="utf-8"><title>Spotifast review evidence</title><a href="pr-1198/">PR #1198 visual comparison</a>\n')
print(intro)
print(json.dumps(comparisons, indent=2))
