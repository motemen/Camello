use strict;
use warnings;
use Sealed;
use Open;
use Factory;

# Every module `Sealed` uses was read or is recognised, so "declares no method"
# is about a closed world.
Sealed->absent;                 #~ warning unknown-method: `Sealed` declares no method `absent`

# `Open` uses something the run never found, which could have installed
# anything. The same sentence, one severity down.
Open->absent;                   #~ info unknown-method: `Open` declares no method `absent`

# On an instance the class is what something *said* the value is, and that is
# only as good as the `new` that made it. `Sealed` takes the framework's, which
# hands back a `Sealed`.
my $sealed = Sealed->new;
$sealed->absent;                #~ warning unknown-method: `Sealed` declares no method `absent`

# `Factory::new` hands back another class, so the same sentence about a value
# annotated `Factory` is a guess — the methods that value answers to are its
# own class's (INFER-2g).
# Returns: Factory
sub make { Factory->new('thing') }
my $made = make();
$made->absent;                  #~ info unknown-method: `Factory` declares no method `absent`

# Named rather than held: `Factory` here is exactly the package asked about,
# and that package really does declare no such method.
Factory->absent;                #~ warning unknown-method: `Factory` declares no method `absent`
