use strict;
use warnings;
use Sealed;
use Open;

# Every module `Sealed` uses was read or is recognised, so "declares no method"
# is about a closed world.
Sealed->absent;                 #~ warning unknown-method: `Sealed` declares no method `absent`

# `Open` uses something the run never found, which could have installed
# anything. The same sentence, one severity down.
Open->absent;                   #~ info unknown-method: `Open` declares no method `absent`
