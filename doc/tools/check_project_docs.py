"""Check Markdown links in the focused repository without legacy diagram tooling."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
files = [ROOT / 'README.md', ROOT / 'CONTRIBUTING.md',
         ROOT / 'apps/desktop/README.md', *sorted((ROOT / 'doc').rglob('*.md'))]
errors = []
for path in files:
    content = path.read_text(encoding='utf-8')
    for target in re.findall(r'!?\[[^\]]*\]\(([^)]+)\)', content):
        if re.match(r'^(?:[a-z]+:|#)', target):
            continue
        target = target.split('#', 1)[0].strip('<>')
        if not (path.parent / target).exists():
            errors.append(f'{path.relative_to(ROOT)}: missing {target}')
if errors:
    raise SystemExit('\n'.join(errors))
print(f'PASS: local links in {len(files)} Markdown files.')
