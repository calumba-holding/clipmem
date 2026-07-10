from __future__ import annotations
import argparse
import re
from pathlib import Path

parser = argparse.ArgumentParser(description='Validate the Clipmem audit Markdown and source line citations')
parser.add_argument('--deliverables', type=Path, default=Path(__file__).resolve().parents[1])
parser.add_argument('--source-root', type=Path, required=True)
args = parser.parse_args()
D=args.deliverables.resolve(); S=args.source_root.resolve()
errors=[]; warnings=[]; links=0; refs=0
link_re=re.compile(r'\[[^\]]*\]\(([^)]+)\)')
span_re=re.compile(r'`([^`\n]+)`')
ref_re=re.compile(r'(?P<path>(?:src|macos|scripts|docs|\.github|skills|plans)/[^;,) ]+?|README\.md|Cargo\.toml|schema\.sql|[A-Za-z0-9_+.-]+\.(?:rs|swift|sql|py|sh|md|yml|yaml|toml)):(?P<start>\d+)(?:-(?P<end>\d+))?')
idx={}
for f in S.rglob('*'):
    if f.is_file(): idx.setdefault(f.name,[]).append(f)
for p in D.rglob('*.md'):
    text=p.read_text()
    if text.count('```') % 2: errors.append(f'unbalanced fenced code block: {p.relative_to(D)}')
    if 'macos/...' in text: errors.append(f'abbreviated source path: {p.relative_to(D)}')
    for raw in link_re.findall(text):
        links += 1
        target=raw.split('#',1)[0].strip()
        if not target or '://' in target or target.startswith(('mailto:','sandbox:')): continue
        if not (p.parent/target).resolve().exists(): errors.append(f'broken link: {p.relative_to(D)} -> {raw}')
    for sm in span_re.finditer(text):
        for m in ref_re.finditer(sm.group(1)):
            refs += 1
            path=m.group('path'); a=int(m.group('start')); b=int(m.group('end') or a)
            if '/' in path:
                candidates=[S/path] if (S/path).exists() else [f for f in S.rglob(Path(path).name) if str(f.relative_to(S)).endswith(path)]
            else: candidates=idx.get(path,[])
            if not candidates: errors.append(f'missing source ref: {p.relative_to(D)} `{path}:{a}-{b}`'); continue
            if len(candidates)>1: warnings.append(f'ambiguous source ref: {p.relative_to(D)} `{path}:{a}-{b}`'); continue
            n=sum(1 for _ in candidates[0].open(errors='replace'))
            if a<1 or b<a or b>n: errors.append(f'bad line range: {p.relative_to(D)} `{path}:{a}-{b}`; file has {n}')
print(f'Markdown files: {len(list(D.rglob("*.md")))}')
print(f'Local/other links inspected: {links}')
print(f'Line-level source citations inspected: {refs}')
print(f'Errors: {len(errors)}')
print(f'Warnings: {len(warnings)}')
for item in errors: print('ERROR:', item)
for item in warnings: print('WARNING:', item)
raise SystemExit(1 if errors else 0)
