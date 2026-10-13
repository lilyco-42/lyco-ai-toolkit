"""CI-only immutable draft storage; never publish or delete existing artifacts."""
import argparse
import glob
import hashlib
import io
import json
import os
from pathlib import Path
import re
import urllib.error
import urllib.request
from urllib.parse import quote
import zipfile

REPO = 'lilyco-42/lyco-ai-toolkit'
LIMIT = 1024 * 1024 * 1024


def sha(data):
    return hashlib.sha256(data).hexdigest()


def validate_draft(release, commit):
    if release.get('draft') is not True or release.get('target_commitish') != commit:
        raise ValueError('Refuse a published release or another source target')


def archive_files(root, patterns):
    root = root.resolve()
    selected = {}
    for pattern in patterns:
        for name in glob.glob(str(root / pattern), recursive=True):
            path = Path(name)
            candidates = list(path.rglob('*')) if path.is_dir() else [path]
            for candidate in candidates:
                if not candidate.is_file():
                    continue
                if candidate.is_symlink() or not candidate.resolve().is_relative_to(root):
                    raise ValueError('Archive input escapes the CI checkout')
                selected[candidate.relative_to(root).as_posix()] = candidate
    if not selected:
        raise ValueError('Required CI output files missing')
    if sum(path.stat().st_size for path in selected.values()) > LIMIT:
        raise ValueError('CI output exceeds draft storage budget')
    output = io.BytesIO()
    records = {}
    with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
        for name, path in sorted(selected.items()):
            data = path.read_bytes()
            archive.writestr(name, data)
            records[name] = {'size': len(data), 'sha256': sha(data)}
    return output.getvalue(), records


def api(path, data=None):
    request = urllib.request.Request('https://api.github.com/repos/' + REPO + '/' + path,
        headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json',
                 'Content-Type': 'application/json'},
        data=json.dumps(data).encode() if data is not None else None)
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)


def draft_by_tag(tag):
    found = []
    page = 1
    while True:
        releases = api('releases?per_page=100&page=' + str(page))
        found.extend(item for item in releases if item['tag_name'] == tag)
        if len(releases) < 100:
            break
        page += 1
    if len(found) > 1:
        raise ValueError('Ambiguous draft identity')
    return found[0] if found else None


def immutable_upload(release, name, data):
    expected = {'name': name, 'size': len(data), 'digest': 'sha256:' + sha(data)}
    matches = [a for a in release['assets'] if a['name'] == name]
    if matches:
        if len(matches) != 1 or any(matches[0].get(k) != v for k, v in expected.items()):
            raise ValueError('Refuse to replace an existing different asset')
        return matches[0]
    url = f'https://uploads.github.com/repos/{REPO}/releases/{release["id"]}/assets?name={quote(name)}'
    request = urllib.request.Request(url, data=data, headers={
        'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json',
        'Content-Type': 'application/octet-stream'})
    with urllib.request.urlopen(request, timeout=180) as response:
        result = json.load(response)
    if any(result.get(k) != v for k, v in expected.items()):
        raise ValueError('GitHub draft asset bytes differ')
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--name', required=True)
    parser.add_argument('--path', action='append', required=True)
    args = parser.parse_args()
    if (os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('GITHUB_REPOSITORY') != REPO
            or not re.fullmatch('[a-z0-9-]+', args.name)):
        raise SystemExit('Draft retention only for this trusted repository Actions')
    if os.environ.get('GITHUB_EVENT_NAME') == 'pull_request':
        event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
        if event['pull_request']['head']['repo']['full_name'] != REPO:
            raise SystemExit('No write credentials for untrusted fork builds')
    commit, run = os.environ['GITHUB_SHA'], os.environ['GITHUB_RUN_ID']
    if not re.fullmatch('[a-f0-9]{40}', commit) or not run.isdigit():
        raise ValueError('Invalid source/run identity')
    data, files = archive_files(Path(os.environ['GITHUB_WORKSPACE']), args.path)
    tag = f'ci-draft-{run}-{commit[:12]}'
    release = draft_by_tag(tag)
    if release is None:
        try:
            release = api('releases', {'tag_name': tag, 'target_commitish': commit,
                'name': 'Unpublished CI evidence ' + run, 'draft': True, 'prerelease': True,
                'body': 'Internal CI evidence only. Not approved or published as a product.'})
        except urllib.error.HTTPError as error:
            if error.code != 422:
                raise
            # Another matrix job may have created this exact draft concurrently.
            release = draft_by_tag(tag)
            if release is None:
                raise ValueError('Concurrent draft creation did not resolve') from None
    validate_draft(release, commit)
    asset = immutable_upload(release, args.name + '.zip', data)
    receipt = {'repo': REPO, 'run': run, 'commit': commit, 'draft_release_id': release['id'],
               'draft': True, 'published': False, 'asset_id': asset['id'],
               'asset_name': asset['name'], 'size': len(data), 'sha256': sha(data), 'files': files}
    fresh = api('releases/' + str(release['id']))
    validate_draft(fresh, commit)
    if not any(a['id'] == asset['id'] and a['digest'] == 'sha256:' + sha(data) for a in fresh['assets']):
        raise ValueError('Stored draft evidence unavailable')
    text = json.dumps(receipt, indent=2)
    print('CI_DRAFT_RECEIPT_BEGIN\n' + text + '\nCI_DRAFT_RECEIPT_END')
    with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as summary:
        summary.write('\n### Unpublished CI evidence\n\n```json\n' + text + '\n```\n')


if __name__ == '__main__':
    main()
