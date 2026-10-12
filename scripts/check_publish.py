"""Check Git-visible files before publication; never print credential contents."""
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    result = subprocess.run(
        ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'],
        cwd=ROOT, capture_output=True, check=True,
    )
    paths = sorted(set(p for p in result.stdout.decode('utf-8').split('\0') if p))
    indexed = set(subprocess.run(
        ['git', 'ls-files', '-z'], cwd=ROOT, capture_output=True, check=True,
    ).stdout.decode('utf-8').split('\0'))
    patterns = {
        'GitHub token': r'\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})\b',
        'Google API key': r'\bAIza[A-Za-z0-9_-]{30,}\b',
        'Google auth key': r'\bAQ\.[A-Za-z0-9_-]{35,}\b',
        'Private key': r'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
        'OAuth secret': r'"(?:client_secret|refresh_token|access_token)"\s*:\s*"[^"\s]{12,}"',
    }
    errors = []
    for name in paths:
        path = ROOT / name
        if not path.is_file():
            if name in indexed:
                errors.append(f'{name}: staged file missing from working copy; stage removal or review it')
            continue
        parts = path.relative_to(ROOT).parts
        if any(p in {'.cache', 'target', 'node_modules', '__pycache__', 'test-results',
                     'playwright-report'} for p in parts):
            errors.append(f'{name}: generated/private directory')
        if (path.name == '.env' or path.name.startswith('.env.') and path.name != '.env.example'
                or path.name in {'client.json', 'token.json', 'credentials.json'}
                or path.name.endswith(('.pem', '.key'))):
            errors.append(f'{name}: private credential path')
        if path.suffix.lower() in {'.exe', '.dll', '.pdb', '.pyc'}:
            errors.append(f'{name}: build artifact')
        if path.stat().st_size > 10 * 1024 * 1024:
            errors.append(f'{name}: oversized source artifact')
        if path.suffix.lower() in {'.png', '.ico'}:
            continue
        try:
            content = path.read_text(encoding='utf-8-sig')
        except UnicodeDecodeError:
            errors.append(f'{name}: unreviewed binary content')
            continue
        contents = [('working copy', content)]
        if name in indexed:
            staged = subprocess.run(
                ['git', 'show', ':' + name], cwd=ROOT, capture_output=True, check=True,
            ).stdout.decode('utf-8-sig')
            contents.append(('staged copy', staged))
        for location, text in contents:
            for label, pattern in patterns.items():
                if re.search(pattern, text):
                    errors.append(f'{name}: possible {label} in {location}; inspect locally')
    if errors:
        raise SystemExit('\n'.join(errors))
    print(f'PASS: {len(paths)} Git-visible files checked; no selected sensitive patterns/artifacts.')


if __name__ == '__main__':
    main()
