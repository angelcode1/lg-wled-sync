#!/usr/bin/env python3
"""C5 Stage 12: bounded offline ELF and upstream source contract inventory.

Does not connect to the TV, call capture, execute a vendor binary, extract
anything or grant permissions. Symbol/ASCII hints NEVER establish a usable
frame lease, sampled-source timestamp, valid client identity, or DRM access.
Python 3.9+ standard library only.
"""
import argparse
import ast
import hashlib
import json
import re
import stat
import sys
import tempfile
import zipfile
from pathlib import Path

MAX_ARCHIVE = 110 * 1024 * 1024
MAX_MEMBER = 30 * 1024 * 1024
MAX_TOTAL = 85 * 1024 * 1024
CANDIDATES = {
    'captureservice': 'captureservice',
    'libvtcapture.so.1.0.0': 'vtcapture',
    'libhalgal.so.2.0.1': 'halgal',
    'lightingcontroller': 'lightingcontroller',
    'livepick': 'livepick',
}
# Counts of case-insensitive search hints, not interpretation of function semantics.
HINTS = {
    'timestamp_text': br'time[ _-]?stamp',
    'monotonic_clock_text': br'CLOCK_MONOTONIC|clock_gettime',
    'frame_sequence_text': br'frame[ _-]?(?:sequence|number|counter)',
    'acquire_text': br'acquire(?:Frame|Buffer|Fence)?',
    'release_text': br'release(?:Frame|Buffer|Fence)?',
    'fence_text': br'\bfence\b|sync_file',
    'capture_reply_text': br'writtenBytes|capturedWidth|capturedHeight',
}


class Refuse(ValueError):
    pass


def regular_file(path, limit):
    meta = path.lstat()
    if not stat.S_ISREG(meta.st_mode) or not (1 <= meta.st_size <= limit):
        raise Refuse('REFUSE_NONREGULAR_OR_SIZE')
    return meta.st_size


def scan_elf_archive(path):
    regular_file(path, MAX_ARCHIVE)
    found = []
    seen = set()
    used = 0
    with zipfile.ZipFile(path, 'r') as archive:
        for info in archive.infolist():
            filename = info.filename.replace('\\', '/')
            parts = filename.split('/')
            if info.is_dir():
                continue
            if not parts or '..' in parts or filename.startswith('/'):
                raise Refuse('REFUSE_UNSAFE_ARCHIVE_PATH')
            base = parts[-1]
            if base not in CANDIDATES:
                continue
            if base in seen:
                raise Refuse('REFUSE_DUPLICATE_RELEVANT_ELF')
            seen.add(base)
            unix_mode = info.external_attr >> 16
            if stat.S_IFMT(unix_mode) == stat.S_IFLNK:
                raise Refuse('REFUSE_ARCHIVED_SYMLINK')
            if info.flag_bits & 0x1:
                raise Refuse('REFUSE_ENCRYPTED_MEMBER')
            if not (4 <= info.file_size <= MAX_MEMBER):
                raise Refuse('REFUSE_MEMBER_SIZE')
            if info.compress_size == 0 or info.file_size > info.compress_size * 250:
                raise Refuse('REFUSE_COMPRESSION_RATIO')
            used += info.file_size
            if used > MAX_TOTAL:
                raise Refuse('REFUSE_TOTAL_UNCOMPRESSED_SIZE')
            with archive.open(info, 'r') as handle:
                data = handle.read(MAX_MEMBER + 1)
            if len(data) != info.file_size or not data.startswith(b'\x7fELF'):
                raise Refuse('REFUSE_NOT_BOUNDED_ELF')
            matches = {key: bool(re.search(pattern, data, re.IGNORECASE))
                       for key, pattern in HINTS.items()}
            found.append({
                'kind': CANDIDATES[base],
                'sha256': hashlib.sha256(data).hexdigest(),
                'bytes': len(data),
                'keyword_presence_only': matches,
                'verified_source_timestamp': False,
                'verified_buffer_lease': False,
                'permission_proven': False,
            })
    return {
        'schema': 'c5-stage12-offline-elf-inventory-v1',
        'scanned_binaries': sorted(found, key=lambda v: v['kind']),
        'decision': 'INCONCLUSIVE_STATIC_HINTS_ONLY',
        'note': 'Matched words/symbols cannot prove call semantics, clock origin, ownership or client rights.',
        'executed_vendor_code': False,
        'accessed_tv': False,
        'capture_authorized': False,
    }


def extract_function(content, name):
    pattern = r'\bint\s+' + re.escape(name) + r'\s*\([^;]*?\)\s*\{'
    m = re.search(pattern, content, re.S)
    if m is None:
        raise Refuse('REFUSE_MISSING_FUNCTION_' + name)
    level = 1
    i = m.end()
    start = i
    while i < len(content) and level:
        if content[i] == '{':
            level += 1
        if content[i] == '}':
            level -= 1
        i += 1
    if level:
        raise Refuse('REFUSE_UNBALANCED_FUNCTION_' + name)
    return content[start:i-1]


def source_contract(repo):
    paths = {
        'vt': repo / 'unicapture/backends/libvtcapture.cpp',
        'hal': repo / 'unicapture/backends/libhalgal.c',
        'descriptor': repo / 'unicapture/unicapture.h',
        'loop': repo / 'unicapture/unicapture.c',
    }
    src = {}
    for label, path in paths.items():
        regular_file(path, 2 * 1024 * 1024)
        src[label] = path.read_text(encoding='utf-8')
    vt_acq = extract_function(src['vt'], 'capture_acquire_frame')
    vt_rel = extract_function(src['vt'], 'capture_release_frame')
    hal_acq = extract_function(src['hal'], 'capture_acquire_frame')
    hal_rel = extract_function(src['hal'], 'capture_release_frame')
    descriptor_match = re.search(r'typedef\s+struct\s+_frame_info\s*\{(.*?)\}\s*frame_info_t\s*;', src['descriptor'], re.S)
    if not descriptor_match:
        raise Refuse('REFUSE_UNKNOWN_FRAME_DESCRIPTOR')
    descriptor = descriptor_match.group(1)
    def no_op(function):
        return bool(re.fullmatch(r'\s*return\s+0\s*;\s*', function))
    facts = {
        'vt_pointer_snapshot': 'vtCapture_currentCaptureBuffInfo' in vt_acq,
        'vt_release_callback_noop': no_op(vt_rel),
        'halgal_capture_call': 'HAL_GAL_CaptureFrameBuffer(' in hal_acq,
        'halgal_release_callback_noop': no_op(hal_rel),
        'descriptor_has_no_timestamp': re.search(r'time[ _-]?stamp|\bts\b|\bpts\b', descriptor, re.I) is None,
        'loop_uses_local_elapsed_ticks': 'frame_acquired = getticks_us()' in src['loop'],
    }
    return {
        'schema': 'c5-stage12-hyperion-source-contract-v1',
        'observations': facts,
        'static_contract_match': all(facts.values()),
        'frame_timestamp_proven': False,
        'native_buffer_lease_proven': False,
        'scope': 'Hyperion wrapper source, not TV driver ABI or service entitlement',
        'capture_authorized': False,
    }


def selftest():
    with tempfile.TemporaryDirectory(prefix='c5-stage12-offline-') as tmp:
        root = Path(tmp)
        p = root / 'test.zip'
        with zipfile.ZipFile(p, 'w', zipfile.ZIP_DEFLATED) as z:
            z.writestr('usr/sbin/captureservice', b'\x7fELF' + b'Timestamp clock_gettime acquireFrame releaseFence writtenBytes')
        good = scan_elf_archive(p)
        assert len(good['scanned_binaries']) == 1
        assert good['scanned_binaries'][0]['keyword_presence_only']['timestamp_text']
        assert good['scanned_binaries'][0]['verified_source_timestamp'] is False
        assert good['scanned_binaries'][0]['verified_buffer_lease'] is False
        with zipfile.ZipFile(p, 'w') as z:
            z.writestr('a/captureservice', b'\x7fELF payload')
            z.writestr('b/captureservice', b'\x7fELF duplicate')
        try:
            scan_elf_archive(p)
        except Refuse as ex:
            assert str(ex) == 'REFUSE_DUPLICATE_RELEVANT_ELF'
        else:
            raise AssertionError('duplicate must fail')
        with zipfile.ZipFile(p, 'w') as z:
            z.writestr('../captureservice', b'\x7fELF payload')
        try:
            scan_elf_archive(p)
        except Refuse as ex:
            assert str(ex) == 'REFUSE_UNSAFE_ARCHIVE_PATH'
        else:
            raise AssertionError('traversal must fail')
        with zipfile.ZipFile(p, 'w') as z:
            z.writestr('usr/sbin/captureservice', b'not-elf')
        try:
            scan_elf_archive(p)
        except Refuse as ex:
            assert str(ex) == 'REFUSE_NOT_BOUNDED_ELF'
        else:
            raise AssertionError('non-ELF must fail')
        link = root / 'link.zip'
        link.symlink_to(p)
        try:
            scan_elf_archive(link)
        except Refuse as ex:
            assert str(ex) == 'REFUSE_NONREGULAR_OR_SIZE'
        else:
            raise AssertionError('symlink must fail')
        assert not re.fullmatch(r'\s*return\s+0\s*;\s*', ' queueBuffer(); return 0; ')
        assert re.fullmatch(r'\s*return\s+0\s*;\s*', ' return 0; ')
        assert extract_function('int capture_release_frame(void *p){ return 0; }', 'capture_release_frame').strip() == 'return 0;'
        assert ast.parse(Path(__file__).read_text(), feature_version=(3, 9))
    print('C5_STAGE12_STATIC_CONTRACT_SELFTEST_PASS static_only=true hints_not_proof=true duplicate_refused=true traversal_refused=true malformed_elf_refused=true symlink_refused=true noop_checked=true python39_syntax=true tv_access=false capture=false')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    group = ap.add_mutually_exclusive_group(required=True)
    group.add_argument('--selftest', action='store_true')
    group.add_argument('--archive', type=Path, help='owner-held existing offline native ZIP (no extraction)')
    group.add_argument('--hyperion', type=Path, help='local checked-out hyperion-webos repository')
    args = ap.parse_args()
    try:
        if args.selftest:
            selftest()
            return 0
        print(json.dumps(scan_elf_archive(args.archive) if args.archive else source_contract(args.hyperion), indent=2, sort_keys=True))
        return 0
    except (Refuse, OSError, zipfile.BadZipFile, UnicodeError) as exc:
        print('C5_STAGE12_REFUSE:', str(exc), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
