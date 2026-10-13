"""Offline checks of LG C5 model-bound histogram profile import."""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from c5_profile_import import convert

class ProfileTests(unittest.TestCase):
    def setUp(self):
        self.model = "OLED65C5PSA"
        self.data = {
            "profile":"custom", "model":self.model,
            "points":[{"position":i+0.25,"region":i+4} for i in range(4)]
        }

    def test_success_requires_exact_model(self):
        result = convert(self.data,self.model)
        self.assertEqual(result["histogram"]["model"],self.model)
        self.assertEqual(result["histogram"]["points"],self.data["points"])
        self.assertEqual(result["capture_mode"],"auto-drm-first")
        with self.assertRaisesRegex(ValueError,"does not match"):
            convert(self.data,"OLED55C5PSA")

    def test_rejects_unmeasured_and_unsupported_profile(self):
        for kind in ("auto","g4","average"):
            with self.subTest(kind=kind),self.assertRaises(ValueError):
                convert(dict(self.data,profile=kind),self.model)
        with self.assertRaisesRegex(ValueError,"edge coverage"):
            convert(dict(self.data,points=[
                {"position":0.1+i*0.2,"region":i} for i in range(4)
            ]),self.model)

    def test_rejects_invalid_points(self):
        for bad in [
            {"position":float("nan"),"region":2},
            {"position":1.25,"region":16},
            {"position":1.25,"region":True},
            {"position":True,"region":2},
        ]:
            points=self.data["points"].copy()
            points[1]=bad
            with self.subTest(bad=bad),self.assertRaises(ValueError):
                convert(dict(self.data,points=points),self.model)

    def test_cli_fragment(self):
        with tempfile.TemporaryDirectory() as folder:
            source=Path(folder)/"calibration.json"
            output=Path(folder)/"fragment.json"
            source.write_text(json.dumps(self.data))
            script=Path(__file__).with_name("c5_profile_import.py")
            result=subprocess.run([sys.executable,str(script),str(source),
                "--model",self.model,"--output",str(output)],
                capture_output=True,text=True,check=False)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(set(json.loads(output.read_text())),
                {"capture_mode","histogram"})

if __name__ == "__main__":
    unittest.main()
