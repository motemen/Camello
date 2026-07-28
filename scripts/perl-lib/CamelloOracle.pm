package CamelloOracle;

# Supplies the context a fixture assumes, so that `perl -c` answers the question
# we actually mean: "is this Perl?" — not "is this Perl, in an empty directory,
# with no modules installed and no features enabled".
#
# Anything still rejected after this really is not Perl.

use strict;
use warnings;

BEGIN {
    # `use Foo::Bar;` resolves to an empty module. A fixture about formatting a
    # `use` statement should not need the module to exist.
    push @INC, sub {
        my (undef, $path) = @_;
        # The stub has to declare itself in the package being loaded, or
        # `use Foo (LIST)` finds no `import` and dies at compile time.
        (my $package = $path) =~ s{\.pm\z}{};
        $package =~ s{/}{::}g;
        my $stub = "package $package;\nsub import { }\nsub unimport { }\n1;\n";
        open my $handle, '<', \$stub or return;
        return $handle;
    };

    # Accept any subroutine or variable attribute. `sub f : switch(10)` is
    # syntactically fine; whether an attribute handler exists is a different
    # question from whether it parses.
    no strict 'refs';
    no warnings 'once';
    *UNIVERSAL::MODIFY_CODE_ATTRIBUTES   = sub { () };
    *UNIVERSAL::MODIFY_SCALAR_ATTRIBUTES = sub { () };
    *UNIVERSAL::MODIFY_ARRAY_ATTRIBUTES  = sub { () };
    *UNIVERSAL::MODIFY_HASH_ATTRIBUTES   = sub { () };
}

# Try::Tiny's exports, so that `try { ... } catch { ... };` reads as the pair of
# function calls it is. Core `try` is a different construct with a different
# shape, and a file written for one is not written for the other — see the
# dialect note in scripts/perl-check.
sub main::try (&;@)     { }
sub main::catch (&;@)   { }
sub main::finally (&;@) { }

1;
