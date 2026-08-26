use strict;
use warnings;

# A `##` comment, so `camello format` keeps it byte for byte — a suppression a
# reformat could damage would be worse than none — and `##` rather than `#`
# because `## no critic` reads the same way and means something else.

my $unread = 1;                 ## camello-disable: unused-variable

## camello-disable: unused-variable
my $also_unread = 2;

# A marker naming a different code leaves this one alone.
my $still_reported = 3;         ## camello-disable: arity
#~ warning unused-variable: `$still_reported`

# `## no critic` is somebody else's comment.
my $reported_too = 4;           ## no critic (ProhibitUnusedVariables)
#~ warning unused-variable: `$reported_too`
