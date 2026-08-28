package Ping;
use strict;
use warnings;
use Pong;

# Mutual recursion: every path goes through the recursion, so every round asks
# the same question and gets the same `Unknown` back — which is the cut the
# design asked for, without a call graph having to be built to find it.
sub ping { return Pong->pong }

1;
