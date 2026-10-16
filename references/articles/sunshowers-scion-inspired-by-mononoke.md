<div class="container headings--one-size">

<div class="header__inner">

<div class="header__logo">

[](../../)

<div class="logo">

sunshowers

</div>

</div>

-   [about](../../about)
-   [work](../../work)

&nbsp;

-   [about](../../about)
-   [work](../../work)

![](data:image/svg+xml;base64,PHN2ZyBjbGFzcz0idGhlbWUtdG9nZ2xlLWRhcmsiIHdpZHRoPSIyMCIgaGVpZ2h0PSIyMCIgdmlld2JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9ImN1cnJlbnRDb2xvciIgc3Ryb2tlLXdpZHRoPSIyIiBzdHJva2UtbGluZWNhcD0icm91bmQiIHN0cm9rZS1saW5lam9pbj0icm91bmQiIGFyaWEtaGlkZGVuPSJ0cnVlIj48Y2lyY2xlIGN4PSIxMiIgY3k9IjEyIiByPSI1Ij48L2NpcmNsZT48bGluZSB4MT0iMTIiIHkxPSIxIiB4Mj0iMTIiIHkyPSIzIj48L2xpbmU+PGxpbmUgeDE9IjEyIiB5MT0iMjEiIHgyPSIxMiIgeTI9IjIzIj48L2xpbmU+PGxpbmUgeDE9IjQuMjIiIHkxPSI0LjIyIiB4Mj0iNS42NCIgeTI9IjUuNjQiPjwvbGluZT48bGluZSB4MT0iMTguMzYiIHkxPSIxOC4zNiIgeDI9IjE5Ljc4IiB5Mj0iMTkuNzgiPjwvbGluZT48bGluZSB4MT0iMSIgeTE9IjEyIiB4Mj0iMyIgeTI9IjEyIj48L2xpbmU+PGxpbmUgeDE9IjIxIiB5MT0iMTIiIHgyPSIyMyIgeTI9IjEyIj48L2xpbmU+PGxpbmUgeDE9IjQuMjIiIHkxPSIxOS43OCIgeDI9IjUuNjQiIHkyPSIxOC4zNiI+PC9saW5lPjxsaW5lIHgxPSIxOC4zNiIgeTE9IjUuNjQiIHgyPSIxOS43OCIgeTI9IjQuMjIiPjwvbGluZT48L3N2Zz4=) ![](data:image/svg+xml;base64,PHN2ZyBjbGFzcz0idGhlbWUtdG9nZ2xlLWxpZ2h0IiB3aWR0aD0iMjAiIGhlaWdodD0iMjAiIHZpZXdib3g9IjAgMCAyNCAyNCIgZmlsbD0ibm9uZSIgc3Ryb2tlPSJjdXJyZW50Q29sb3IiIHN0cm9rZS13aWR0aD0iMiIgc3Ryb2tlLWxpbmVjYXA9InJvdW5kIiBzdHJva2UtbGluZWpvaW49InJvdW5kIiBhcmlhLWhpZGRlbj0idHJ1ZSI+PHBhdGggZD0iTTIxIDEyLjc5QTkgOSAwIDExMTEuMjEgMyA3IDcgMCAwMDIxIDEyLjc5eiI+PC9wYXRoPjwvc3ZnPg==)

</div>

<div class="content">

<div class="reading-progress" aria-hidden="true">

</div>

<div class="post">

# [Anthropic’s Scion is heavily inspired by Mononoke](../../posts/anthropic-mononoke/)

<div class="post-meta">

2026-10-14 · 2 min read [vcs](https://sunshowers.io/tags/vcs/) [shorts](https://sunshowers.io/tags/shorts/)

</div>

<div class="post-body">

<div class="post-content">

<div>

*Discuss on [Hacker News](https://news.ycombinator.com/item?id=50094694) or [Lobsters](https://lobste.rs/s/plcgps/anthropic_s_scion_is_heavily_inspired_by).*

Here is Anthropic [describing](https://www.anthropic.com/engineering/redesigning-source-control) ([archive link](https://ghostarchive.org/archive/hCFVA)) yesterday how their new version control system, Scion, uses a concept called “derived data” to make operations like blames more efficient:

![How Anthropic describes Scion’s derived data.](../../images/scion-derived-data.png)

Here is me, back in 2022, describing in my Oxide candidate materials how the [Mononoke](https://github.com/facebook/sapling/tree/main/eden/mononoke) source control server I helped build uses a concept called “derived data” to make operations like blames more efficient:

![How I described Mononoke’s derived data.](../../images/mononoke-derived-data.png)

I think the level of imitation goes well beyond having similar insights^([1](#fn:1)). The fact that Sapling was only credited for “hard-won lessons”, and that Mononoke was not mentioned at all, borders on plagiarism. The post comes off as Anthropic claiming that Claude designed all this, while the reality is that our (extraordinary) team at Meta came up with many of the insights that Claude dutifully applied almost a decade later. (We looked at plenty of prior art for Mononoke and Sapling too, from Google’s SrcFS to write-ahead logs in databases.)

I am very glad that the vision our team had back in 2017-18 was so powerful and enduring — when you go against the grain, you’re always a bit unsure if what you’re doing is genius or madness, and I think Mononoke’s design has stood the test of time. And, to be clear, I do not have a problem with Anthropic copying Mononoke’s design. (This is part of why we put out open source — to benefit humanity through the spread of good ideas.)

But as LLMs continue to assist in designing new systems, they are likely to index heavily on existing designs. Mononoke is [open source](https://github.com/facebook/sapling/tree/main/eden/mononoke) and in every LLM’s training set. If you ask [Opus 5.5](https://claude.ai/share/e6a66ec5-7757-4adb-901f-0e33b1c38536) or [GPT-6 Astra](https://chatgpt.com/share/6ace7be6-68f8-83ea-ad68-0f9d14448318) (thanks [Matthew Fowle](https://bsky.app/profile/jauntywunderkind.bsky.social/post/3mxsettjnoc22)) about prior art, they both immediately point out the similarities to Mononoke.

Giving credit to those that came before us is now easier — and more important — than ever. There is no longer an excuse.

<div class="footnotes" role="doc-endnotes">

------------------------------------------------------------------------

1.  <div id="fn:1">

    For an example of a system with similar insights but many key design differences, see [Vicent Marti’s blog post for Cursor](https://cursor.com/blog/git-at-any-scale). [↩︎](#fnref:1)

    </div>

</div>

</div>

</div>

</div>

<div class="pagination">

<div class="pagination__title">

Read other posts

------------------------------------------------------------------------

</div>

<div class="pagination__buttons">

[iddqd, or the hardest kind of unsafe Rust →](../../posts/iddqd-unsafe/)

</div>

</div>

</div>

</div>

<div class="footer__signoff">

[sunshowers](../../) Created with love in Oakland, CA.

</div>

<div class="footer__inner">

<div class="copyright copyright--user">

© Rain 2020-present. Licensed under [CC BY-NC-SA 4.0](https://creativecommons.org/licenses/by-nc-sa/4.0) unless marked otherwise.

</div>

<div class="footer__social">

<div>

[![Mastodon logo](../../images/mastodon-logo.svg)](https://hachyderm.io/@rain)

</div>

<div>

[![Atom (feed) logo](../../images/bluesky-logo.svg)](https://bsky.app/profile/sunshowers.io)

</div>

<div>

[![GitHub logo](../../images/github-logo.svg)](https://github.com/sunshowers)

</div>

<div>

[![Atom (feed) logo](../../images/rss-logo.svg)](https://sunshowers.io/atom.xml)

</div>

</div>

</div>

</div>
