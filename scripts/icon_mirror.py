"""Mirror icon URLs off hosts we do not control.

Most icons come from `render.guildwars2.com` (ArenaNet's own render service)
or `wiki.guildwars2.com`. Both are durable enough to hot-link: the render
service is first-party, and the wiki is heavily mirrored and long-lived.

Two hosts are not in that category. GW2EI sources some art from
`i.imgur.com` -- anonymous uploads made years ago by contributors, which
imgur may remove at any time and has removed before -- and a little from
`assets.gw2dat.com`, a community site with no durability guarantee. Those
are the only source for the art in question: of the 61 ids they back, the
official API knows one, and that one has no icon. So the art cannot simply
be re-sourced; it has to be mirrored.

The mirror lives in the `axibridge-map-tiles` GitHub Pages repo, which
already hosts GW2 map art. Filenames keep their upstream id and gain a host
prefix (`imgur-nSYuby8.png`) so provenance stays readable in a `git diff`.

An unknown URL on a mirrored host is a HARD ERROR, not a pass-through. If
GW2EI adds a 19th imgur icon, regeneration stops and names the file to
mirror. Falling back to the upstream URL would silently reintroduce exactly
the dependency this module exists to remove, and nobody would notice until
the image 404'd in a published report.
"""

import os
from urllib.parse import urlparse

MIRROR_BASE = "https://darkharasho.github.io/axibridge-map-tiles/icons"

#: Hosts we mirror, mapped to the filename prefix they get.
MIRRORED_HOSTS = {
    "i.imgur.com": "imgur",
    "assets.gw2dat.com": "gw2dat",
}

#: Every file currently present in the mirror. Kept explicit so a new
#: upstream URL fails loudly rather than pointing at a 404.
MIRRORED = frozenset(
    """
    gw2dat-3568389.png gw2dat-3568391.png gw2dat-3568392.png
    gw2dat-3691067.png gw2dat-3772576.png imgur-03RLBaX.png
    imgur-0EnjyQX.png imgur-0VuijTx.png imgur-1jUOMlX.png
    imgur-1uDdNtU.png imgur-1znO8HP.png imgur-2B73rSk.png
    imgur-2m630qZ.png imgur-2ybEpCV.png imgur-4wTs28o.png
    imgur-7TAlNtd.png imgur-A6JTWBV.png imgur-ArLGcWu.png
    imgur-Cd9yD43.png imgur-FXgZQ46.png imgur-FnLyZvk.png
    imgur-Glb39dj.png imgur-GqKocpf.png imgur-HbDL75f.png
    imgur-Ie4y9Qf.png imgur-IimHVxe.png imgur-K7taOUe.png
    imgur-LgfmRM4.png imgur-O7kekkb.png imgur-PwhIT4u.png
    imgur-Q96yagv.png imgur-R1f6iXn.png imgur-R5p9fqw.png
    imgur-RiCJalE.png imgur-S8msdHU.png imgur-SjSb5yJ.png
    imgur-TOsmJOl.png imgur-Ti4NWys.png imgur-UbvyFSt.png
    imgur-Wp4lhTM.png imgur-X463V90.png imgur-Z4YUAvW.png
    imgur-aXVbVl6.png imgur-byOtZxM.png imgur-dNY6e8n.png
    imgur-dS8un97.png imgur-e0IXt8w.png imgur-ejI5STj.png
    imgur-fL88z7p.png imgur-hKBqtWE.png imgur-hckhnZy.png
    imgur-iEpKYL0.jpg imgur-kK3l1C1.png imgur-kryyJRy.png
    imgur-l329bR4.png imgur-l7SjOSw.png imgur-lvp7545.png
    imgur-lxeruPM.png imgur-mFzTJXv.png imgur-nAaynHA.png
    imgur-nNQEVpb.png imgur-nSYuby8.png imgur-nVAyYVQ.png
    imgur-nVu2ivF.png imgur-pIFrNLa.png imgur-qaXHsQU.png
    imgur-r7TAcjS.png imgur-rI1tW64.png imgur-sncfljQ.png
    imgur-t0khtQd.png imgur-u8l36Pw.png imgur-uVdgw3H.png
    imgur-uf1VZEJ.png imgur-whOAxsp.png imgur-xRdE1iN.png
    """.split()
)


def mirror(url):
    """`url` rewritten to the mirror, or returned unchanged if it is already
    on a host we trust."""
    prefix = MIRRORED_HOSTS.get(urlparse(url).netloc)
    if prefix is None:
        return url
    name = f"{prefix}-{os.path.basename(urlparse(url).path)}"
    if name not in MIRRORED:
        raise SystemExit(
            f"\n{url}\n"
            f"  is on a mirrored host but is not in the mirror yet.\n"
            f"  Upload it to axibridge-map-tiles as icons/{name}, add that\n"
            f"  name to MIRRORED in scripts/icon_mirror.py, and re-run.\n"
        )
    return f"{MIRROR_BASE}/{name}"
