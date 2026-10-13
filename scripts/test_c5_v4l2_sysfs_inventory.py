import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name('c5_v4l2_sysfs_inventory.sh')


class SysfsInventoryTest(unittest.TestCase):
    def test_missing_sysfs_is_noninvasive(self):
        with tempfile.TemporaryDirectory() as root:
            env = dict(os.environ, C5_V4L2_SYSFS_ROOT=str(Path(root) / 'missing'),
                       C5_V4L2_DEVICE=str(Path(root) / 'missingdev'),
                       C5_V4L2_MARKER=str(Path(root) / 'missingmarker'))
            result = subprocess.run(['sh', str(SCRIPT)], capture_output=True,
                                    text=True, env=env, check=True)
            self.assertIn('VIDEO60_DEVICE_ABSENT', result.stdout)
            self.assertIn('VIDEO60_SYSFS_UNAVAILABLE', result.stdout)
            self.assertIn('ioctls=0', result.stdout)

    def test_fake_sysfs_reports_identity_without_open(self):
        with tempfile.TemporaryDirectory() as root:
            fake = Path(root) / 'video60'
            fake.mkdir()
            (fake / 'dev').write_text('81:60\n')
            (fake / 'name').write_text('test-capture-card\n')
            (fake / 'uevent').write_text('MAJOR=81\nMINOR=60\n')
            env = dict(os.environ, C5_V4L2_SYSFS_ROOT=str(fake),
                       C5_V4L2_DEVICE='/dev/null',
                       C5_V4L2_MARKER=str(Path(root) / 'missingmarker'))
            result = subprocess.run(['sh', str(SCRIPT)], capture_output=True,
                                    text=True, env=env, check=True)
            self.assertIn('81:60', result.stdout)
            self.assertIn('test-capture-card', result.stdout)
            self.assertIn('C5_V4L2_SYSFS_INVENTORY_COMPLETE', result.stdout)
            self.assertIn('stream_actions=0', result.stdout)


if __name__ == '__main__':
    unittest.main()
