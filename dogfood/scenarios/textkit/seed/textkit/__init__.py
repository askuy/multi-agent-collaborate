"""textkit: small text utilities, implemented by a team of agents."""

from textkit.dedupe import dedupe
from textkit.duration import parse_duration
from textkit.roman import roman
from textkit.slugify import slugify
from textkit.word_count import word_count
from textkit.wrap import wrap

__all__ = ["dedupe", "parse_duration", "roman", "slugify", "word_count", "wrap"]
