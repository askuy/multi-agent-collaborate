"""pipeline: a small text pipeline whose functions build on each other.

normalize <- slugify, tokenize <- top_words <- summarize (each also uses the ones it names).
"""
