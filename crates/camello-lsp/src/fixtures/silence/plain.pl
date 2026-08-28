use strict;
use warnings;

my $thing = shift @ARGV;
#   ^ hover $thing : Unknown
print $thing->whatever;
#             ^ complete-own -
#             ^ hover whatever -> Unknown
#            ^ hover -
