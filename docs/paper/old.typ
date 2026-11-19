#import "@preview/cetz:0.4.2": canvas, draw
#set page(width: 12cm, height: auto, margin: (x: 1.4cm, y: 1.2cm))
#set text(size: 10pt)
#set par(justify: true)
#set heading(numbering: "1.")
#set math.equation(numbering: "(1)")
#counter(heading).update(1)

= Damped oscillations

A pendulum released from a small angle swings with an amplitude that decays slowly over time. For weak damping, its angle follows

$ theta(t) = theta_0 e^(-gamma t) cos(omega t) $ <decay>

where $gamma$ is the damping rate.

#figure(
  canvas(length: 1cm, {
    import draw: *
    let f(t) = calc.exp(-0.25 * t) * calc.cos(3 * t)
    line((0, 0), (6.2, 0), stroke: 0.5pt + gray)
    line((0, -1.1), (0, 1.1), stroke: 0.5pt + gray)
    line(..range(121).map(i => (i / 20, f(i / 20))), stroke: 1pt)
    content((6.2, -0.3), $t$)
    content((-0.3, 1), $theta$)
  }),
  caption: [Measured angle.],
) <fit>

Air drag is small at these speeds. We fitted @decay to 250 samples from the video recording.
