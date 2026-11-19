= Results

The measurements were taken on a board with a 64 MHz processor, averaged over 50 runs.

@speed shows the time per sort. Insertion sort is fastest up to about 40 numbers, heap sort above that.

#figure(
  image("../figures/speed.svg", width: 65%),
  caption: [Time per sort, for lists of 10 to 1000 numbers.],
) <speed>
