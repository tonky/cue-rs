#!/usr/bin/env python3
"""Measure Odoo variants sequentially. Run under `just safe` for memory caps."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('repro', type=Path)
parser.add_argument('--binary', type=Path)
parser.add_argument('--runs', type=int, default=3)
parser.add_argument('--reference-dir', type=Path,
                    help='saved upstream refs.upstream.json and literal.upstream.json')
parser.add_argument('--output', type=Path, default=Path('tmp/odoo-benchmark.json'))
args = parser.parse_args()
if args.runs < 1:
    parser.error('--runs must be positive')
if args.binary is None:
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--no-deps', '--offline', '--format-version', '1']))
    args.binary = Path(metadata['target_directory']) / 'release/cue-rs'
binary = args.binary.resolve(strict=True)
repro = args.repro.resolve(strict=True)
Path('tmp').mkdir(exist_ok=True)
source_hash = hashlib.sha256()
for source in sorted(repro.rglob('*.cue')):
    source_hash.update(str(source.relative_to(repro)).encode())
    source_hash.update(b'\0')
    source_hash.update(source.read_bytes())
report = {'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
          'repro': str(repro), 'cue_sources_sha256': source_hash.hexdigest(),
          'reference_checked': args.reference_dir is not None, 'cases': {}}
with tempfile.TemporaryDirectory(prefix='odoo-benchmark-', dir='tmp') as temporary:
    directory = Path(temporary).resolve()
    for case in ['literal', 'refs', 'original']:
        samples = []
        first_value = None
        for index in range(args.runs):
            output = directory / 'output.json'
            measurement = directory / 'measurement.json'
            with output.open('w') as stdout:
                run = subprocess.run([
                    '/usr/bin/time', '-q', '-f', '{"seconds":%e,"rss_kib":%M}',
                    '-o', str(measurement), str(binary), 'eval', './' + case,
                ], cwd=repro, stdout=stdout, stderr=subprocess.PIPE, text=True, timeout=90)
            expected_exit = 1 if case == 'original' else 0
            if run.returncode != expected_exit:
                raise SystemExit(f'{case}: unexpected exit {run.returncode}: {run.stderr[:1000]}')
            sample = json.loads(measurement.read_text())
            sample['exit_code'] = run.returncode
            if case != 'original':
                value = json.loads(output.read_text())
                if index == 0:
                    first_value = value
                elif value != first_value:
                    raise SystemExit(f'{case}: result changed between samples')
                if args.reference_dir:
                    expected = json.loads((args.reference_dir / (case + '.upstream.json')).read_text())
                    if value != expected:
                        raise SystemExit(f'{case}: differs from saved upstream JSON')
                canonical = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
                sample['json_sha256'] = hashlib.sha256(canonical).hexdigest()
            else:
                sample['diagnostic'] = run.stderr.strip()
            samples.append(sample)
            print(f'{case} {index+1}/{args.runs}: {sample["seconds"]:.2f} s, '
                  f'{sample["rss_kib"]} KiB, exit {run.returncode}', flush=True)
        report['cases'][case] = {'samples': samples,
            'median_seconds': statistics.median(s['seconds'] for s in samples),
            'median_rss_kib': statistics.median(s['rss_kib'] for s in samples),
            'max_rss_kib': max(s['rss_kib'] for s in samples)}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2) + '\n')
print(f'Report: {args.output}')
