"""Actions-only review of existing full AIC+Spine bytes; never rebuild products."""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import urllib.error
import urllib.request
from urllib.parse import urlsplit
import zipfile

REPO = 'lilyco-42/lyco-ai-toolkit'
APP_REPO = 'lilyco-42/lilyco'
APP = 'a308ad03671a2b8d415b85ff2d7e2e7bfc42796b'
SOURCE = '336cddb47b340d2c33a6ef3041a438d28bbd1dad'
SOURCE_RUN = 38340868199
PREFIX = 'Lain42-Asset-Studio/'
LIMIT = 200 * 1024 * 1024


def sha(data):
    return hashlib.sha256(data).hexdigest()


def github(path, repo=REPO):
    if repo not in {REPO, APP_REPO}:
        raise ValueError('Unrelated repository')
    request = urllib.request.Request('https://api.github.com/repos/' + repo + '/' + path,
        headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json'})
    with urllib.request.urlopen(request, timeout=45) as response:
        return json.load(response)


def validate_run(run):
    expected = {'head_sha': SOURCE, 'head_branch': 'main', 'event': 'push',
                'name': 'Licensed Asset Studio', 'status': 'completed', 'conclusion': 'success'}
    if any(run.get(k) != v for k, v in expected.items()) or run['head_repository']['full_name'] != REPO:
        raise ValueError('Only the successful formal full licensed product may be reviewed')


def required_flags(receipt, flags):
    if any(receipt.get(name) is not True for name in flags):
        raise ValueError('Missing actual behavior evidence')


def artifact_bytes(item):
    if item['expired'] or item['workflow_run']['id'] != SOURCE_RUN:
        raise ValueError('Artifact not from the exact source run')
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *args):
            return None
    request = urllib.request.Request(f'https://api.github.com/repos/{REPO}/actions/artifacts/{item["id"]}/zip',
        headers={'Authorization': 'Bearer ' + os.environ['GH_TOKEN'], 'Accept': 'application/vnd.github+json'})
    try:
        urllib.request.build_opener(NoRedirect()).open(request, timeout=45)
        raise ValueError('Expected official artifact redirect')
    except urllib.error.HTTPError as error:
        if error.code != 302:
            raise ValueError('Artifact retrieval refused') from None
        location = error.headers['Location']
    parsed = urlsplit(location)
    if parsed.scheme != 'https' or not parsed.hostname.endswith('.blob.core.windows.net'):
        raise ValueError('Unexpected artifact origin')
    with urllib.request.urlopen(location, timeout=120) as response:
        data = response.read(LIMIT + 1)
    if len(data) > LIMIT or 'sha256:' + sha(data) != item['digest']:
        raise ValueError('Outer artifact digest differs')
    return data


def members(data):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        files = [item for item in archive.infolist() if not item.is_dir()]
        if sum(item.file_size for item in files) > LIMIT:
            raise ValueError('Excessive artifact expansion')
        result = {}
        for item in files:
            path = PurePosixPath(item.filename)
            if path.is_absolute() or '..' in path.parts or item.filename in result:
                raise ValueError('Unsafe or duplicate artifact path')
            result[item.filename] = archive.read(item)
        return result


def inspect_package(data, platform):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        files = [item for item in archive.infolist() if not item.is_dir()]
        names = {item.filename for item in files}
        if len(names) != len(files) or sum(item.file_size for item in files) > LIMIT:
            raise ValueError('Duplicate or excessive package')
        for name in names:
            path = PurePosixPath(name)
            if path.is_absolute() or '..' in path.parts or not name.startswith(PREFIX):
                raise ValueError('Unsafe package path')
        manifest = json.loads(archive.read(PREFIX + 'manifest.json'))
        if (manifest['commit'] != APP or manifest['spine_source_commit'] != SOURCE
                or manifest['spine_runtime_revision'] != '4ff8b7280aeb7367f235752c30dc85184c01452a'
                or manifest['contains_spine_runtime'] is not True or manifest['runner'] != platform
                or manifest['runtime_license'] != 'Spine Runtimes License Agreement'):
            raise ValueError('Wrong or incomplete original engine sources')
        release_id = hashlib.sha1((APP + '\n' + SOURCE).encode()).hexdigest()
        if manifest['release_id'] != release_id:
            raise ValueError('Full product release identity differs')
        if names != {PREFIX + name for name in manifest['files']} | {PREFIX + 'manifest.json'}:
            raise ValueError('Manifest does not cover the full package')
        for name, digest in manifest['files'].items():
            if sha(archive.read(PREFIX + name)) != digest:
                raise ValueError('Packaged file differs: ' + name)
        suffix = '.exe' if platform == 'Windows' else ''
        for name in ['_internal/bin/laic' + suffix, '_internal/bin/lyco-spine-render' + suffix]:
            body = archive.read(PREFIX + name)
            if len(body) < 1000 or not body.startswith(b'MZ' if suffix else b'\x7fELF'):
                raise ValueError('Original native engine missing')
        notices = json.loads(archive.read(PREFIX + 'THIRD-PARTY-NOTICES.json'))
        agreement = archive.read(PREFIX + 'SPINE-RUNTIME-LICENSE.html').decode('utf-8')
        runtime = [item for item in notices if item.get('ecosystem') == 'Spine Runtimes']
        if (len(runtime) != 1 or runtime[0]['notices']['current-agreement'] != agreement
                or 'Spine Runtimes License Agreement' not in agreement or len(agreement) < 1000
                or 'not MIT' not in runtime[0]['license']):
            raise ValueError('Full distinct Spine license missing')
        for name in ('LICENSE', 'SPINE-TOOL-SOURCE-LICENSE', 'SPINE-Cargo.lock', 'AIC-Cargo.lock'):
            if len(archive.read(PREFIX + name)) < 100:
                raise ValueError('Required source attribution missing')
        if json.loads(archive.read(PREFIX + 'UI-package-lock.json'))['version'] != '0.1.1':
            raise ValueError('Wrong desktop product version')
        if platform == 'Windows':
            pin = json.loads(Path('public-app/apps/asset-studio/packaging/webview2-sdk.json').read_text())
            original = Path('public-app/apps/asset-studio/MICROSOFT-WEBVIEW2-LICENSE.txt').read_bytes()
            with urllib.request.urlopen(pin['source'], timeout=60) as response:
                sdk_data = response.read(40 * 1024 * 1024 + 1)
            if sha(sdk_data) != pin['package_sha256']:
                raise ValueError('Official SDK package changed')
            with zipfile.ZipFile(io.BytesIO(sdk_data)) as sdk:
                if sdk.read(pin['license_path']) != original or archive.read(PREFIX + 'MICROSOFT-WEBVIEW2-LICENSE.txt') != original:
                    raise ValueError('Full original SDK license differs')
                for item in pin['files']:
                    if (sha(sdk.read(item['sdk_path'])) != item['sha256']
                            or sha(archive.read(PREFIX + '_internal/webview/lib/' + item['app_path'])) != item['sha256']):
                        raise ValueError('Official SDK DLL differs')
        return {'release_id': release_id, 'manifest_files': len(manifest['files']),
                'full_spine_notice_verified': True, 'both_original_native_engines_present': True}


def main():
    if os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('GITHUB_REPOSITORY') != REPO:
        raise SystemExit('Byte review only in this repository Actions')
    validate_run(github('actions/runs/' + str(SOURCE_RUN)))
    subprocess.run(['git', 'diff', '--exit-code', SOURCE, 'HEAD', '--', 'crates', 'Cargo.toml', 'Cargo.lock',
                    '.github/workflows/asset-studio-licensed.yml'], check=True)
    for repo, commit, expected in [(REPO, SOURCE, {'ci', 'Licensed Asset Studio'}),
                                 (APP_REPO, APP, {'CI', 'Asset Studio', 'Vision Studio Desktop'})]:
        latest = {}
        for run in sorted(github('actions/runs?head_sha=' + commit + '&per_page=100', repo)['workflow_runs'], key=lambda x: x['id'], reverse=True):
            if run['head_branch'] == 'main' and run['event'] == 'push':
                latest.setdefault(run['name'], run)
        if not expected <= latest.keys() or any(latest[name]['status'] != 'completed' or latest[name]['conclusion'] != 'success' for name in expected):
            raise ValueError('Both exact source repositories require all formal gates')
    jobs = github(f'actions/runs/{SOURCE_RUN}/jobs?per_page=100')['jobs']
    if len(jobs) != 2 or any(j['status'] != 'completed' or j['conclusion'] != 'success' for j in jobs):
        raise ValueError('Both real product build jobs must have passed')
    required_steps = {'Real AIC and Spine animation/export, isolation and cancellation',
                      'Exercise both bundled engines from the actual package'}
    for job in jobs:
        success = {s['name'] for s in job['steps'] if s['conclusion'] == 'success'}
        if not required_steps <= success:
            raise ValueError('Missing actual original-engine behavior gates')
        if 'windows' in job['name'].lower() and not {'Verify real packaged WebView2, export and normal close',
                'Build and exercise the Windows installation EXE'} <= success:
            raise ValueError('Missing real native or installed engine gates')
    artifacts = github(f'actions/runs/{SOURCE_RUN}/artifacts?per_page=100')['artifacts']
    output = Path('reviewed-release')
    output.mkdir(exist_ok=True)
    reviews, assets = [], []
    for platform in ('Windows', 'Linux'):
        selected = [a for a in artifacts if a['name'] == f'asset-studio-licensed-{platform}-{SOURCE}' and not a['expired']]
        if len(selected) != 1:
            raise ValueError('Exact formal artifact missing or ambiguous')
        files = members(artifact_bytes(selected[0]))
        filename = 'lain42-asset-studio-' + platform.lower() + '.zip'
        package = files['dist/' + filename]
        digest = sha(package)
        receipt = json.loads(files['dist/package-evidence.json'])
        if (files['dist/' + filename + '.sha256'].decode().strip() != digest or receipt['sha256'] != digest
                or receipt['app_commit'] != APP or receipt['spine_source_commit'] != SOURCE
                or receipt['contains_spine_runtime'] is not True or receipt['size'] != len(package)):
            raise ValueError('Product bytes differ from formal receipt')
        record = inspect_package(package, platform)
        distributable = {filename: package, filename + '.sha256': files['dist/' + filename + '.sha256']}
        if platform == 'Windows':
            setup = 'lain42-asset-studio-windows-setup.exe'
            installer = files['dist/' + setup]
            build = json.loads(files['evidence/installer-build.json'])
            if (build['commit'] != APP or build['spine_source_commit'] != SOURCE
                    or build['sha256'] != sha(installer) or build['size'] != len(installer)
                    or files['dist/' + setup + '.sha256'].decode().strip() != sha(installer)):
                raise ValueError('Installer identity differs')
            required_flags(json.loads(files['evidence/installer-acceptance.json']),
                ['installed_files_sha256_verified', 'installed_app_real_engine', 'uninstall_removed_packaged_files',
                 'unrelated_file_preserved', 'start_menu_removed', 'real_uninstaller_exit_checked', 'original_uninstaller_removed'])
            native = json.loads(files['evidence/windows-native.json'])
            if native['commit'] != APP:
                raise ValueError('Wrong native app source receipt')
            required_flags(native, ['native_webview2', 'real_aic_render', 'real_export_zip', 'normal_close', 'workspace_removed', 'port_closed'])
            distributable.update({setup: installer, setup + '.sha256': files['dist/' + setup + '.sha256']})
        else:
            for label in ('desktop', 'mobile', 'compact', 'tablet'):
                if not files['evidence/spine-' + label + '.png'].startswith(b'\x89PNG'):
                    raise ValueError('Actual responsive Spine evidence missing')
        for name, data in distributable.items():
            (output / name).write_bytes(data)
            assets.append({'name': name, 'size': len(data), 'sha256': sha(data)})
        reviews.append(dict(record, platform=platform, outer_artifact_digest=selected[0]['digest'],
                            actual_behavior_gates_verified=True))
    result = {'app_commit': APP, 'spine_source_commit': SOURCE, 'source_run': SOURCE_RUN,
              'records': reviews, 'assets': assets, 'automatic_publication': False,
              'user_confirmed_applicable_spine_editor_authorization': True}
    (output / 'review.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({'reviewed_complete_combo': True, 'assets': len(assets), 'publicly_published': False}))


if __name__ == '__main__':
    main()
