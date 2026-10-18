"""The code palette keeps the system's rules: readable everywhere code is drawn, greys achromatic, poison and cloak out of the tokens."""
import copy
import json
import math
from pathlib import Path
import unittest

from syntax import resolve
from theme import contrast, generate, oklch

HERE = Path(__file__).parent
COLOR = json.loads((HERE / "color.json").read_text())
SYNTAX = json.loads((HERE / "syntax.json").read_text())
HOSTS = ("background", "float", "cursorline", "selection", "search", "diff-added", "diff-removed",
         "diff-changed", "diff-added-text", "diff-removed-text", "diff-changed-text")
GREYS = ("fg", "structure", "quiet")
HUES = ("string", "constant", "function", "type")


def hue_distance(a, b):
    d = abs(math.degrees(oklch(a)[2]) - math.degrees(oklch(b)[2])) % 360
    return min(d, 360 - d)


class SyntaxTests(unittest.TestCase):
    def check(self, color_source, syntax_source):
        color = generate(color_source)
        result = resolve(syntax_source, color_source, color)
        for mode in ("night", "day"):
            syntax, ui = result[mode]["syntax"], result[mode]["ui"]
            for role, ink in syntax.items():
                for host in HOSTS:
                    self.assertGreaterEqual(contrast(ink, ui[host]), result["contrast"], (mode, role, host))
            for role in ("match", "line-number", "line-number-current", "danger", "ok", "warn", "info"):
                self.assertGreaterEqual(contrast(ui[role], ui["background"]), result["contrast"], (mode, role))
            self.assertGreaterEqual(contrast(ui["on-search-current"], ui["search-current"]), result["contrast"], mode)
            for role, spec in syntax_source["roles"].items():
                if spec["tone"] in GREYS:
                    ink = syntax[role]
                    self.assertTrue(ink[1:3] == ink[3:5] == ink[5:7], (mode, role, ink))
            # Even colour: the hues share one lightness.
            levels = [oklch(syntax[spec["tone"]])[0] for spec in syntax_source["roles"].values() if spec["tone"] in HUES]
            self.assertLess(max(levels) - min(levels), .01, mode)
            # Structure sits between the names and the punctuation.
            fg, structure, quiet = (oklch(syntax[role])[0] for role in ("text", "keyword", "punctuation"))
            if mode == "night":
                self.assertTrue(fg > structure > quiet, mode)
            else:
                self.assertTrue(fg < structure < quiet, mode)
        return color, result

    def test_nightshade(self):
        color, result = self.check(COLOR, SYNTAX)
        for mode in ("night", "day"):
            # Poison is the search match and the live signal, never a token's ink;
            # cloak is the cursor and the bracket under it.
            for role, ink in result[mode]["syntax"].items():
                self.assertGreater(hue_distance(ink, color[mode]["primary"]), 25, (mode, role, ink)) if oklch(ink)[1] > .02 else None
                self.assertGreater(hue_distance(ink, color[mode]["accent"]), 25, (mode, role, ink)) if oklch(ink)[1] > .02 else None
            self.assertEqual(result[mode]["ui"]["search-current"], color[mode]["primary"])
            self.assertEqual(result[mode]["ui"]["cursor"], color[mode]["accent"])

    def test_other_themes_and_contrast_levels(self):
        for base, accent in (("{cloak}", "{poison}"), ("#0066ff", "#ff5500"), ("#ffffff", "#000000")):
            for strength in (0, 50, 100):
                with self.subTest(base=base, accent=accent, contrast=strength):
                    source = copy.deepcopy(COLOR)
                    source["theme"].update(base=base, accent=accent, contrast=strength)
                    self.check(source, SYNTAX)

    def test_does_not_mutate_the_sources(self):
        color, syntax = copy.deepcopy(COLOR), copy.deepcopy(SYNTAX)
        resolve(syntax, color, generate(color))
        self.assertEqual((color, syntax), (COLOR, SYNTAX))


if __name__ == "__main__":
    unittest.main()
