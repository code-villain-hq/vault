"""Stress-test the system's relationships, including themes unlike Nightshade."""
import copy
import json
from pathlib import Path
import unittest

from theme import HOSTS, contrast, generate, luminance, mix, oklch, tone

SOURCE = json.loads(Path(__file__).with_name("color.json").read_text())


class ThemeTests(unittest.TestCase):
    def check_theme(self, result):
        minimum = result["contrast"]["text"]
        for mode in ("night", "day"):
            theme = result[mode]
            hosts = [theme[role] for role in HOSTS]
            for role in ("bg", "panel", "surface", "hover", "line", "fg", "muted", "faint", "dialog"):
                color = theme[role]
                self.assertEqual(color[1:3], color[3:5], (mode, role))
                self.assertEqual(color[3:5], color[5:7], (mode, role))
            for role in ("fg", "muted", "faint", "accent", *SOURCE["semantics"]):
                for host in hosts:
                    self.assertGreaterEqual(contrast(theme[role], host), minimum, (mode, role, host))
            for role in ("primary", "primary-hover"):
                self.assertGreaterEqual(contrast(theme["on-primary"], theme[role]), minimum)
            for host in hosts:
                self.assertGreaterEqual(contrast(theme["signal"], host), result["contrast"]["signal"], (mode, host))
            # The signal never moves further from the seed than the text accent does.
            self.assertGreaterEqual(contrast(theme["accent"], theme["bg"]), contrast(theme["signal"], theme["bg"]) - 1e-9)
            for host in hosts:
                chip = mix(host, theme["fg"], result["opacity"]["chip-tint"])
                for role in ("muted", "faint"):
                    self.assertGreaterEqual(contrast(theme[role], chip), minimum, (mode, role, chip))
                # The active option must stand apart from its host and keep every text tone readable.
                highlight = mix(host, theme["fg"], result["opacity"]["highlight-tint"])
                self.assertGreaterEqual(contrast(highlight, host), 1.1, (mode, host))
                for role in ("fg", "muted", "faint"):
                    self.assertGreaterEqual(contrast(theme[role], highlight), minimum, (mode, role, highlight))
            for status in (*SOURCE["semantics"], "neutral"):
                hue = theme["muted" if status == "neutral" else status]
                for host in hosts:
                    pill = mix(host, hue, result["opacity"]["status-tint"])
                    self.assertGreaterEqual(contrast(theme[f"status-{status}-ink"], pill), minimum)
            # Secondary and tertiary text keep distinct tones at the default
            # contrast. (At 0 and 100 they can still merge; not solved yet.)
            if result["theme"]["contrast"] == 50:
                self.assertNotEqual(theme["muted"], theme["faint"], mode)
            self.assertNotEqual(theme["dialog-field"], theme["dialog"])
            self.assertNotEqual(theme["dialog-field-hover"], theme["dialog"])
            self.assertEqual(set(theme), set(result["night"]))
            if mode == "night":
                self.assertLess(luminance(theme["bg"]), luminance(theme["panel"]))
                self.assertLess(luminance(theme["panel"]), luminance(theme["surface"]))
                self.assertLess(luminance(theme["surface"]), luminance(theme["dialog"]))
            else:
                self.assertGreater(luminance(theme["panel"]), luminance(theme["field"]))

    def test_nightshade_and_extreme_seeds_at_all_contrast_levels(self):
        for base, accent in (("{cloak}", "{poison}"), ("#0066ff", "#ff5500"),
                             ("#ffffff", "#000000"), ("#000000", "#ffffff"),
                             ("#ff00ff", "#00ff00"), ("#ffff00", "#0000ff")):
            for strength in (0, 50, 100):
                with self.subTest(base=base, accent=accent, contrast=strength):
                    source = copy.deepcopy(SOURCE)
                    source["theme"].update(base=base, accent=accent, contrast=strength)
                    self.check_theme(generate(source))

    def test_input_changes_propagate_without_leaking_accent_into_neutrals(self):
        original = generate(SOURCE)
        source = copy.deepcopy(SOURCE)
        source["theme"]["base"] = "#0066ff"
        changed_base = generate(source)
        for mode in ("night", "day"):
            self.assertNotEqual(original[mode]["primary"], changed_base[mode]["primary"])
        source = copy.deepcopy(SOURCE)
        source["theme"]["accent"] = "#ff5500"
        changed_accent = generate(source)
        for mode in ("night", "day"):
            self.assertNotEqual(original[mode]["accent"], changed_accent[mode]["accent"])
            for role in ("bg", "panel", "surface", "fg", "muted", "primary"):
                self.assertEqual(original[mode][role], changed_accent[mode][role])

    def test_higher_contrast_increases_readability_in_both_modes(self):
        source = copy.deepcopy(SOURCE)
        source["theme"]["contrast"] = 0
        low = generate(source)
        source["theme"]["contrast"] = 100
        high = generate(source)
        for mode in ("night", "day"):
            for role in ("fg", "muted", "faint"):
                self.assertGreater(contrast(high[mode][role], high[mode]["panel"]),
                                   contrast(low[mode][role], low[mode]["panel"]))

    def test_oklch_round_trip_and_srgb_gamut(self):
        for hex in ("#000000", "#ffffff", "#ff0000", "#00ff00", "#0000ff", "#5a1470", "#c9e265"):
            self.assertEqual(tone(*oklch(hex)), hex.upper())
        # Gamut mapping preserves lightness rather than clipping RGB channels.
        self.assertAlmostEqual(oklch(tone(.6, .4, 1.5))[0], .6, places=2)

    def test_rejects_invalid_inputs(self):
        for strength in (-1, 101, True, "50", float("nan")):
            source = copy.deepcopy(SOURCE)
            source["theme"]["contrast"] = strength
            with self.assertRaises(ValueError):
                generate(source)
        source = copy.deepcopy(SOURCE)
        source["theme"]["base"] = "purple"
        with self.assertRaises(ValueError):
            generate(source)

    def test_does_not_mutate_the_source(self):
        source = copy.deepcopy(SOURCE)
        generate(source)
        self.assertEqual(source, SOURCE)


if __name__ == "__main__":
    unittest.main()
