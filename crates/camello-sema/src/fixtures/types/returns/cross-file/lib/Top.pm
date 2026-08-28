package Top;
use strict;
use warnings;
use Middle;

# Three unannotated subs, three files, and the rounds are the depth of the
# chain rather than the number of files.
sub get {
    my ($self) = @_;
    return Middle->fetch;
}

1;
