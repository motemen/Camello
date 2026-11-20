use strict;
use warnings;

my $i;
for $i (1 .. 3) {        #~ warning non-lexical-loop-iterator
    print $i;
}
foreach $i (1 .. 3) {    #~ warning non-lexical-loop-iterator
    print $i;
}
for my $j (1 .. 3) {
    print $j;
}
for our $k (1 .. 3) {
    print $k;
}
for (1 .. 3) {
    print $_;
}
for (my $n = 0; $n < 3; $n++) {
    print $n;
}
