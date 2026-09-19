import subprocess, base64, json, sys, re
from datetime import datetime, timezone, timedelta

REPO = '18163623522/dcc-plugin-manager'
BRANCH = 'main'

def gh(endpoint, method='GET', payload=None):
    cmd = ['gh', 'api', '--method', method, endpoint]
    if payload is not None:
        cmd += ['--input', '-']
    r = subprocess.run(cmd, input=json.dumps(payload) if payload is not None else None,
                       capture_output=True, text=True, encoding='utf-8')
    if r.returncode != 0:
        print('API FAIL', method, endpoint, '\n', r.stderr[:400], file=sys.stderr)
        sys.exit(1)
    return json.loads(r.stdout) if r.stdout.strip() else {}

def git(*args, binary=False):
    r = subprocess.run(['git', *args], capture_output=True)
    return r.stdout if binary else r.stdout.decode('utf-8')

uploaded = set()

def iso_date(ts, tz):  # "1789854700" "+0800" -> "2026-09-19T21:05:47+08:00"
    sign = 1 if tz[0] == '+' else -1
    delta = timedelta(hours=int(tz[1:3]), minutes=int(tz[3:5]))
    return datetime.fromtimestamp(int(ts), tz=timezone(sign * delta)).isoformat(timespec='seconds')

def build_tree(tree_sha):
    """按 tree SHA 递归上传（名字相对本层），返回该层 tree 的 API 对象"""
    items = []
    for line in git('ls-tree', tree_sha).splitlines():
        meta, name = line.split('\t', 1)
        mode, typ, sha = meta.split()
        if typ == 'blob':
            if sha not in uploaded:
                content = git('cat-file', 'blob', sha, binary=True)
                b = gh(f'repos/{REPO}/git/blobs', 'POST',
                       {'content': base64.b64encode(content).decode(), 'encoding': 'base64'})
                uploaded.add(b['sha'])
            items.append({'path': name, 'mode': mode, 'type': 'blob', 'sha': sha})
        elif typ == 'tree':
            sub = build_tree(sha)
            items.append({'path': name, 'mode': mode, 'type': 'tree', 'sha': sub['sha']})
    return gh(f'repos/{REPO}/git/trees', 'POST', {'tree': items})

commits = git('rev-list', '--reverse', 'HEAD').split()
print('commits to push:', len(commits))

parent = None
for c in commits:
    raw = git('cat-file', 'commit', c)
    message = raw.split('\n\n', 1)[1]

    def person(kind):
        m = re.search(rf'^{kind} (.*) <(.*)> (\d+) ([+-]\d{{4}})$', raw, re.M)
        return {'name': m.group(1), 'email': m.group(2),
                'date': iso_date(m.group(3), m.group(4))}

    root_tree_sha = git('rev-parse', f'{c}^{{tree}}').strip()
    tree = build_tree(root_tree_sha)
    payload = {'message': message, 'tree': tree['sha'],
               'author': person('author'), 'committer': person('committer')}
    if parent:
        payload['parents'] = [parent]
    commit = gh(f'repos/{REPO}/git/commits', 'POST', payload)
    print(f'  {c[:8]} -> server {commit["sha"][:8]} {"SAME" if commit["sha"] == c else "DIFF"}')
    parent = commit['sha']

gh(f'repos/{REPO}/git/refs/heads/{BRANCH}', 'PATCH', {'sha': parent, 'force': True})
print('ref updated:', BRANCH, parent)
