<div id="smallhead">

<div id="smallhead-inner">

# [Simon Willison’s Weblog](/)

[Newsletter](/newsletters/)

</div>

</div>

<div id="sponsored-banner" class="sponsor-scheme-sky">

<div id="sponsored-banner-inner">

**Sponsored by:** Pomerium — Static OAuth scopes won't stop AI agent breakouts. [Upgrade to Pomerium's agentic access management trusted by NVIDIA and OpenAI](https://fandf.co/4hJeKAm)

</div>

</div>

<div id="wrapper">

<div id="primary">

<div class="entry entryPage">

13th October 2026 - Link Blog

<div permalink-context="/2026/Oct/13/redesigning-source-control-at-anthropic/">

**[Redesigning source control at Anthropic](https://www.anthropic.com/engineering/redesigning-source-control)** ([via](https://news.ycombinator.com/item?id=50073633 "Hacker News")) Interesting piece from Anthropic describing why and how they built their own custom alternative to Git, based on the dramatic increase in rate of changes to their code after the past six months.

> In May 2026, Anthropic averaged around 2,500 code changes merged per day; as of September, we merged over 30,000 per day.

They included a chart showing those changes. I [had Claude modify their chart](https://claude.ai/share/aefcdd4a-9364-40c1-927a-29bf445894fc) ([code here](https://gist.github.com/simonw/4a16dff066fd9763faf8642f797905be)) to add markers for the public releases of each of their models in 2026:

![Stacked bar chart titled “Changes merged per day”, for the Anthropic monorepo, January–September 2026. Each day’s bar is split into pull requests (blue) and antique change requests (orange), with a black line for the 7-day average. Weekdays start at around 1,000 merges in January and climb slowly to about 3,000 by late May. Antique change requests appear in late May, overtake pull requests in mid-June and have almost fully replaced them by mid-July, when weekdays reach 6,000–8,000. A labelled “Quiet week” at the end of July dips below 1,000 a day. After that, growth turns steep: over 10,000 a day by mid-August, about 17,000 in early September, and a peak of 30,401 on Sep 9. Dotted lines mark Claude model releases: Opus 4.6 (Feb 5), Sonnet 4.6 (Feb 17), Mythos Preview (Apr 7), Opus 4.7 (Apr 16), Opus 4.8 (May 28), Fable 5 and Mythos 5 (Jun 9), Sonnet 5 (Jun 30), Opus 5 (Jul 24, just before the quiet week), and Fable 5.1 and Mythos 5.1 (Sep 1).](https://static.simonwillison.net/static/2026/anthropic-merges-per-day.webp)

This shows public release dates, not the date when models became available for Anthropic's internal use. My guess is that the introduction of Fable/Mythos class models is a key factor in the increased coding velocity.

"Antique" is their new code review system. The source control system is called Scion, and sounds to me like a reinvention of centralized source control along the lines of Subversion.

(**Update**: Anthropic credit Google's [Piper](https://en.wikipedia.org/wiki/Piper_(source_control_system)) and Meta's [Sapling](https://sapling-scm.com/) as "prior art" - it looks like the core concept of "derived data" [was heavily inspired by Mononoke](https://sunshowers.io/posts/anthropic-mononoke/), the server component of Sapling.)

They had some pretty extreme requirements:

> The history of our main branch alone is almost 100 GB, and a *compressed* working copy is over 25 GB. We perform on the order of 25 million code checkouts per day for CI, before counting developer environments, agent sandboxes, and research jobs. Fetched naively, that would be over 600 PB of git traffic per day.

The new system was built by a team of seven people, and the migration from Git was completed in five months.

When asked on Hacker News about the lack of offline support, Scion engineer Nick G [replied](https://news.ycombinator.com/item?id=50073633#50079495):

> Since Claude is by far the heaviest user of Scion and is necessarily online, this just wasn't important to do.

As an aside, I've been trying adversarial review in my own projects recently, where you have a model try and find flaws (I often play Claude and GPT off against each other). This note from the article reflected my own experience with that:

> Anthropic also makes extensive use of adversarial review, and, while this is incredibly effective in many ways, it can also be myopic, so skilled engineers needed to steer Claude away from over-engineering parts of the system in response to hypothetical concerns.

</div>

<div class="entryFooter">

Posted [13th October 2026](/2026/Oct/13/) at 11 pm

</div>

</div>

<div class="recent-articles">

## Recent articles

-   [Hello Python 3.15, goodbye 3.10, and now I can use 3.11](/2026/Oct/12/python-3-15/) - 12th October 2026
-   [A new feature for my blog, built using my voice](/2026/Oct/9/built-using-my-voice/) - 9th October 2026
-   [Claude Haiku 5.5](/2026/Oct/7/claude-haiku-5-5/) - 7th October 2026
-   [We're going to need default hard budget caps on pretty much everything](/2026/Oct/3/default-hard-budget-caps/) - 3rd October 2026

</div>

</div>

<div id="secondary">

<div class="metabox">

This is a **link post** by Simon Willison, posted on [13th October 2026](/2026/Oct/13/).

[git 58](/tags/git/) [migrations 15](/tags/migrations/) [version-control 13](/tags/version-control/) [ai 2,281](/tags/ai/) [generative-ai 2,023](/tags/generative-ai/) [llms 1,989](/tags/llms/) [anthropic 348](/tags/anthropic/) [agentic-engineering 66](/tags/agentic-engineering/)

<div class="section" style="
    /* .promo */
    border-radius: 8px;
    margin: 1.5rem 0;
    padding: 1rem 1.25rem;
    /* .variant-a */
    border: 2px solid #6c3eb9;
  ">

### Monthly briefing

Sponsor me for **$10/month** and get a curated email digest of the month's most important LLM developments.

Pay me to send you less!

[Sponsor & subscribe](https://github.com/sponsors/simonw/)

</div>

</div>

</div>

</div>

<div id="ft">

-   [Disclosures](/about/#disclosures)
-   [Colophon](/about/#about-site)
-   ©
-   [2002](/2002/)
-   [2003](/2003/)
-   [2004](/2004/)
-   [2005](/2005/)
-   [2006](/2006/)
-   [2007](/2007/)
-   [2008](/2008/)
-   [2009](/2009/)
-   [2010](/2010/)
-   [2011](/2011/)
-   [2012](/2012/)
-   [2013](/2013/)
-   [2014](/2014/)
-   [2015](/2015/)
-   [2016](/2016/)
-   [2017](/2017/)
-   [2018](/2018/)
-   [2019](/2019/)
-   [2020](/2020/)
-   [2021](/2021/)
-   [2022](/2022/)
-   [2023](/2023/)
-   [2024](/2024/)
-   [2025](/2025/)
-   [2026](/2026/)
-   ![](data:image/svg+xml;base64,PHN2ZyBpZD0iaWNvbi1hdXRvIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdib3g9IjAgMCAyNCAyNCI+PHBhdGggZD0iTTEyIDIyQzYuNDc3IDIyIDIgMTcuNTIzIDIgMTJTNi40NzcgMiAxMiAyczEwIDQuNDc3IDEwIDEwLTQuNDc3IDEwLTEwIDEwem0wLTJWNGE4IDggMCAxIDAgMCAxNnoiPjwvcGF0aD48L3N2Zz4=) ![](data:image/svg+xml;base64,PHN2ZyBpZD0iaWNvbi1saWdodCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIiB2aWV3Ym94PSIwIDAgMjQgMjQiIHN0eWxlPSJkaXNwbGF5Om5vbmUiPjxwYXRoIGQ9Ik0xMiAxOGE2IDYgMCAxIDEgMC0xMiA2IDYgMCAwIDEgMCAxMnptMC0yYTQgNCAwIDEgMCAwLTggNCA0IDAgMCAwIDAgOHpNMTEgMWgydjNoLTJWMXptMCAxOWgydjNoLTJ2LTN6TTMuNTE1IDQuOTI5bDEuNDE0LTEuNDE0TDcuMDUgNS42MzYgNS42MzYgNy4wNSAzLjUxNSA0Ljkzek0xNi45NSAxOC4zNjRsMS40MTQtMS40MTQgMi4xMjEgMi4xMjEtMS40MTQgMS40MTQtMi4xMjEtMi4xMjF6bTIuMTIxLTE0Ljg1bDEuNDE0IDEuNDE1LTIuMTIxIDIuMTIxLTEuNDE0LTEuNDE0IDIuMTIxLTIuMTIxek01LjYzNiAxNi45NWwxLjQxNCAxLjQxNC0yLjEyMSAyLjEyMS0xLjQxNC0xLjQxNCAyLjEyMS0yLjEyMXpNMjMgMTF2MmgtM3YtMmgzek00IDExdjJIMXYtMmgzeiI+PC9wYXRoPjwvc3ZnPg==) ![](data:image/svg+xml;base64,PHN2ZyBpZD0iaWNvbi1kYXJrIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdib3g9IjAgMCAyNCAyNCIgc3R5bGU9ImRpc3BsYXk6bm9uZSI+PHBhdGggZD0iTTEwIDdhNyA3IDAgMCAwIDEyIDQuOXYuMWMwIDUuNTIzLTQuNDc3IDEwLTEwIDEwUzIgMTcuNTIzIDIgMTIgNi40NzcgMiAxMiAyaC4xQTYuOTc5IDYuOTc5IDAgMCAwIDEwIDd6bS02IDVhOCA4IDAgMCAwIDE1LjA2MiAzLjc2MkE5IDkgMCAwIDEgOC4yMzggNC45MzggNy45OTkgNy45OTkgMCAwIDAgNCAxMnoiPjwvcGF0aD48L3N2Zz4=)

</div>
