# OmegaScans

Original Rust implementation of [Omega Scans](https://omegascans.org), its public catalog/chapter API at api.omegascans.org, and server-rendered Next Flight data. No third-party extension code is imported.

Supports comic discovery, title search, genre filters, newest/updated/popular orders, series details and free image chapters. Paid chapters, login, coin purchases and text novels are outside this initial version. Paid access is reported rather than circumvented. Unnumbered season markers and announcements are omitted from the chapter list; numbered chapters retain their source numbers and order.

Flight records are parsed as bounded JSON data; remote JavaScript is never evaluated. Metadata is cached for five minutes, series and genres for thirty minutes. Covers and pages are brokered through the declared media host.

HTTP 429 remains a cooldown error. Visible site challenges use Manko's browser handoff. Tests use synthetic metadata and original sample reading text, including duplicates, caching, invalid paths and malformed responses. Local release 0.1.0 is prepared; publishing is separate.
