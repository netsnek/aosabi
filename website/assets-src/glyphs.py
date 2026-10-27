# SPDX-License-Identifier: MIT
"""Shared glyph sprite. Every glyph is drawn here, 24 by 24, stroke only, currentColor.
Also the packet helpers. Every SVG that uses xlink:href declares both namespaces."""

XMLNS = 'xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"'

G = {
 # carriers. The globe is Network, whatever protocol it carries
 'http':   '<circle cx="12" cy="12" r="8.5"/><ellipse cx="12" cy="12" rx="3.6" ry="8.5"/><path d="M3.5 12h17M5 7.5h14M5 16.5h14"/>',
 'unix':   '<path d="M9 4.5H6.5a2.5 2.5 0 0 0-2.5 2.5v10a2.5 2.5 0 0 0 2.5 2.5H9M15 4.5h2.5A2.5 2.5 0 0 1 20 7v10a2.5 2.5 0 0 1-2.5 2.5H15"/><circle cx="9.2" cy="12" r="1.5"/><circle cx="14.8" cy="12" r="1.5"/><path d="M10.7 12h2.6"/>',
 'usb':    '<rect x="2.5" y="8" width="19" height="8" rx="4"/><path d="M7 12h10"/>',
 'bt':     '<circle cx="12" cy="12" r="1.8" fill="currentColor" stroke="none"/><path d="M8.2 8.2a5.4 5.4 0 0 0 0 7.6M15.8 8.2a5.4 5.4 0 0 1 0 7.6M5.2 5.4a9.4 9.4 0 0 0 0 13.2M18.8 5.4a9.4 9.4 0 0 1 0 13.2"/>',
 # Serial Line: a D-sub serial connector, drawn with four pins over three in its trapezoid so it
 # stays legible at 16 px. A real DE-9 has five over four
 'serial': '<path d="M4.3 7.5h15.4a1.2 1.2 0 0 1 1.17 1.46l-1.45 6.6A1.8 1.8 0 0 1 17.66 17H6.34a1.8 1.8 0 0 1-1.76-1.44l-1.45-6.6A1.2 1.2 0 0 1 4.3 7.5z"/><path d="M7.5 10.6h.01M10.5 10.6h.01M13.5 10.6h.01M16.5 10.6h.01M9 13.8h.01M12 13.8h.01M15 13.8h.01"/>',
 # LoRa: an antenna mast with a chirp beside it, a wave whose period shortens, which is how the
 # LoRa modulation spreads a symbol. The arcs of Bluetooth radiate, the chirp runs, so the two
 # radio links stay apart at 16 px
 'lora':   '<path d="M5.5 20.5V9"/><circle cx="5.5" cy="6.2" r="2.3"/><path d="M9.2 12C9.80 5.08 13.60 5.08 14.20 12 C14.64 18.92 17.46 18.92 17.90 12 C18.22 5.08 20.28 5.08 20.60 12"/>',
 # the layers
 'braces': '<path d="M9.5 3.5c-2.2 0-3.3 1.1-3.3 3.3v2.4c0 1.5-.8 2.5-2.4 2.8 1.6.3 2.4 1.3 2.4 2.8v2.4c0 2.2 1.1 3.3 3.3 3.3M14.5 3.5c2.2 0 3.3 1.1 3.3 3.3v2.4c0 1.5.8 2.5 2.4 2.8-1.6.3-2.4 1.3-2.4 2.8v2.4c0 2.2-1.1 3.3-3.3 3.3"/>',
 'hash':   '<path d="M9 3.5 7.5 20.5M16.5 3.5 15 20.5M4 9h16.5M3.5 15H20"/>',
 'mail':   '<rect x="3" y="5.5" width="18" height="13" rx="2.5"/><path d="m3.5 7.5 8.5 6.2 8.5-6.2"/>',
 'bits':   '<rect x="4" y="7" width="5.5" height="10" rx="2.75"/><path d="M15.5 7v10M13.2 7h2.3M13.2 17h4.6"/>',
 'frame':  '<path d="M4 8V5.5A1.5 1.5 0 0 1 5.5 4H8M16 4h2.5A1.5 1.5 0 0 1 20 5.5V8M20 16v2.5a1.5 1.5 0 0 1-1.5 1.5H16M8 20H5.5A1.5 1.5 0 0 1 4 18.5V16"/><path d="M8.5 12h7"/>',
 # nodes
 'server': '<rect x="3.5" y="4" width="17" height="6.5" rx="2"/><rect x="3.5" y="13.5" width="17" height="6.5" rx="2"/><path d="M7 7.25h.01M7 16.75h.01M11 7.25h6M11 16.75h6"/>',
 'laptop': '<rect x="5" y="5" width="14" height="10" rx="2"/><path d="M2.5 19h19M9.5 19l.5-1.5h4l.5 1.5"/>',
 'player': '<rect x="6" y="3.5" width="12" height="17" rx="3"/><rect x="8.5" y="6" width="7" height="6.5" rx="1.5"/><circle cx="12" cy="16.4" r="1.6"/>',
 'window': '<rect x="3" y="4.5" width="18" height="15" rx="3"/><path d="M3 9h18"/><circle cx="9" cy="14.2" r="2.4"/><path d="M14 13h4M14 16h3"/>',
 'speaker':'<path d="M4 9.5h3l4.5-3.8v12.6L7 14.5H4z"/><path d="M15 9.2a4 4 0 0 1 0 5.6M17.8 6.6a7.6 7.6 0 0 1 0 10.8"/>',
 'note':   '<path d="M9 17.5V6l9-2v11.5"/><circle cx="6.8" cy="17.5" r="2.3"/><circle cx="15.8" cy="15.5" r="2.3"/>',
 'button': '<rect x="4" y="13" width="16" height="6.5" rx="2"/><path d="M8 13V9.5a4 4 0 0 1 8 0V13"/><path d="M12 3v2M6.2 5l1.2 1.4M17.8 5l-1.2 1.4"/>',
 # thesis row
 'answer': '<path d="M4.5 6.5a2.5 2.5 0 0 1 2.5-2.5h10a2.5 2.5 0 0 1 2.5 2.5v7a2.5 2.5 0 0 1-2.5 2.5H11l-4.5 4v-4a2.5 2.5 0 0 1-2-2.5z"/><path d="M8.5 9h7M8.5 12h4.5"/>',
 'stack':  '<path d="m12 3.5 8.5 4.3L12 12 3.5 7.8z"/><path d="m3.5 12 8.5 4.2 8.5-4.2M3.5 16.2l8.5 4.3 8.5-4.3"/>',
 'card':   '<rect x="5.5" y="3" width="13" height="18" rx="2.5"/><path d="M8.5 15.5h7M8.5 18h4"/><path d="M12 12.2 9.4 9.6a1.7 1.7 0 0 1 2.4-2.4l.2.2.2-.2a1.7 1.7 0 0 1 2.4 2.4z"/>',
 # steps and misc
 'down':   '<path d="M12 4v11M7.5 11 12 15.5 16.5 11M5 19.5h14"/>',
 'folder': '<path d="M3.5 7a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>',
 'turn':   '<path d="M4.5 11a7.6 7.6 0 0 1 13.4-4M18.5 3.5v3.9h-3.9M19.5 13a7.6 7.6 0 0 1-13.4 4M5.5 20.5v-3.9h3.9"/>',
 'go':     '<path d="M5 12h13M13 6.5 18.5 12 13 17.5"/>',
 'lr':     '<path d="M3.5 12h17M8 7.5 3.5 12 8 16.5M16 7.5l4.5 4.5L16 16.5"/>',
 'kernel': '<rect x="6.5" y="6.5" width="11" height="11" rx="2"/><path d="M9.5 3.5v3M14.5 3.5v3M9.5 17.5v3M14.5 17.5v3M3.5 9.5h3M3.5 14.5h3M17.5 9.5h3M17.5 14.5h3"/>',
 # the pink cross of the mark, as a kicker ornament. Filled, not stroked.
 'x':      '<g transform="rotate(45 12 12)"><path d="M9.6 1.6h4.8v8h8v4.8h-8v8H9.6v-8h-8V9.6h8z" fill="currentColor" stroke="none"/></g>',
}

def sprite():
    out = [f'<svg {XMLNS} width="0" height="0" style="position:absolute" aria-hidden="true" focusable="false"><defs>']
    for k, v in G.items():
        out.append(f'<symbol id="g-{k}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">{v}</symbol>')
    # shared paint for the card pictures and the wires
    out.append('''
<pattern id="p-dots" width="14" height="14" patternUnits="userSpaceOnUse"><circle cx="2" cy="2" r="1" fill="#e0d6c8"/></pattern>
<linearGradient id="gr-stage" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f8f4ee"/><stop offset="1" stop-color="#efe8de"/></linearGradient>
<linearGradient id="gr-wire" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#c98654"/><stop offset=".45" stop-color="#27c7d1"/><stop offset="1" stop-color="#27c7d1"/></linearGradient>
<linearGradient id="gr-copper" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#e0a577"/><stop offset="1" stop-color="#b8713f"/></linearGradient>
<filter id="f-glow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="5"/></filter>
''')
    out.append('</defs></svg>')
    return ''.join(out)

def use(name, x, y, size=24, color=None, extra=''):
    c = f' color="{color}"' if color else ''
    return f'<use href="#g-{name}" xlink:href="#g-{name}" x="{x}" y="{y}" width="{size}" height="{size}"{c}{extra}/>'

def icon(name, cls='gl'):
    return f'<svg class="{cls}" {XMLNS} viewBox="0 0 24 24" aria-hidden="true"><use href="#g-{name}" xlink:href="#g-{name}"/></svg>'

# ---------------------------------------------------------------- the three packet kinds
# One definition for the whole site. A figure names a kind, never a radius, a colour or a marker,
# so a packet looks the same in every drawing and in every legend. A request is the solid pink dot,
# a response the smaller solid pink dot, a push the pink ring with the white centre, drawn so that
# its outer edge is the size of a request. The push figure calls a request a cause and a push the
# event, which changes the words of that legend and not the glyph.
PINK = '#db178c'
KINDS = {
    'request':  {'r': 5.0, 'ring': False, 'cls': ''},      # solid, large
    'response': {'r': 3.6, 'ring': False, 'cls': 'sm'},    # solid, small
    'push':     {'r': 3.9, 'ring': True,  'cls': 'ring'},  # ring, outer edge 5.0
}
ORDER = ('request', 'response', 'push')                     # the fixed order of every legend


def _n(v):
    return f'{v:.2f}'.rstrip('0').rstrip('.')


def shape(kind):
    """The marker of one packet kind, centred on the origin. Also the still marker of that kind."""
    k = KINDS[kind]
    if k['ring']:
        return (f'<circle r="{_n(k["r"] + 4)}" fill="{PINK}" opacity=".14"/>'
                f'<circle r="{_n(k["r"])}" fill="#fff" stroke="{PINK}" stroke-width="2.2"/>')
    return (f'<circle r="{_n(k["r"] * 2.1)}" fill="{PINK}" opacity=".16"/>'
            f'<circle r="{_n(k["r"])}" fill="{PINK}"/>')


def legend(*items):
    """The packet legend of a figure header. An item is a kind, or a kind with the words that
    figure uses for it. A legend lists only the kinds its figure draws, always in ORDER."""
    pairs = [(i, i) if isinstance(i, str) else i for i in items]
    pairs = [(k, w[:1].upper() + w[1:]) for k, w in pairs]    # a legend entry starts with a capital
    pairs.sort(key=lambda p: ORDER.index(p[0]))
    return ''.join('<span><i class="lg%s"></i>%s</span>'
                   % ((' ' + KINDS[k]['cls']) if KINDS[k]['cls'] else '', w) for k, w in pairs)


def pkt(path, dur, begin=0, kind='request', rev=False):
    """One packet on a drawn path, on its own loop. rev runs it backwards along that path, which is
    the direction and not the kind: a cause that travels from a window to the server rides the
    server to window path backwards and is still a request. The fade envelope hides the loop seam."""
    kp = ' keyPoints="1;0" keyTimes="0;1" calcMode="linear"' if rev else ''
    return (f'<g class="pkt" opacity="0">{shape(kind)}'
            f'<animateMotion dur="{dur}s" begin="{begin}s" repeatCount="indefinite"{kp}><mpath href="#{path}" xlink:href="#{path}"/></animateMotion>'
            f'<animate attributeName="opacity" dur="{dur}s" begin="{begin}s" repeatCount="indefinite" values="0;1;1;0" keyTimes="0;.06;.94;1"/></g>')


def pkt_window(path, cycle, t0, t1, kind='request', rev=False):
    """A packet that runs once inside the window t0..t1 (fractions) of one shared cycle, and waits
    off the picture for the rest of it. Every packet of a figure begins at 0 s on the same cycle,
    so the schedule of a figure is one list of windows that can be read for overlaps."""
    e = 0.012
    a, b = (1, 0) if rev else (0, 1)
    t0m = max(t0, 0.0005)
    kt = f'0;{t0m:.4f};{t1:.4f};1'
    kp = f'{a};{a};{b};{b}'
    ot = f'0;{t0m:.4f};{t0m+e:.4f};{t1-e:.4f};{t1:.4f};1'
    ov = '0;0;1;1;0;0'
    return (f'<g class="pkt" opacity="0">{shape(kind)}'
            f'<animateMotion dur="{cycle}s" begin="0s" repeatCount="indefinite" calcMode="linear" keyPoints="{kp}" keyTimes="{kt}"><mpath href="#{path}" xlink:href="#{path}"/></animateMotion>'
            f'<animate attributeName="opacity" dur="{cycle}s" begin="0s" repeatCount="indefinite" values="{ov}" keyTimes="{ot}"/></g>')


def pkt_at(path, cycle, begin, travel, kind='request', rev=False):
    """The same packet, with the schedule written in seconds: it leaves at begin and arrives at
    begin plus travel, inside one cycle. Two packets on one path never share an interval."""
    return pkt_window(path, cycle, begin / cycle, (begin + travel) / cycle, kind, rev)


def still(x, y, kind='request'):
    """A static packet, shown only under prefers-reduced-motion, so the direction stays readable."""
    return f'<g class="still" transform="translate({_n(x)} {_n(y)})">{shape(kind)}</g>'
