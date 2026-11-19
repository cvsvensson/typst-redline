#set page(width: 12cm, height: auto, margin: (x: 1.5cm, y: 1.2cm))
#set text(size: 10pt)
#set par(justify: true)
#set heading(numbering: "1.")

= Setup

The pendulum hangs from a steel pivot fixed to the ceiling, about two metres above the floor.

A camera above the bob records its position at 120 frames per second, and a light gate checks the timing once per swing.

The string is 1.20 m long and the bob weighs 0.5 kg. We release it by burning a thread, so that it starts without a push.

= Analysis

We track the bob in each frame and convert its position to an angle. The first two seconds are left out, while the thread still swings.
