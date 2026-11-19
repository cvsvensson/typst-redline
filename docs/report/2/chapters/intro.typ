= Introduction

Small devices sort data all the time: a sensor node orders its readings before it sends the median, and a keyboard controller orders the keys pressed at once. Their memory is measured in kilobytes, so an algorithm that needs a second copy of the data is often ruled out.

We measured the time that three algorithms take on lists of up to 1000 numbers. Merge sort needs extra memory, so we used the in-place variant of Katajainen et al. @katajainen1996.
