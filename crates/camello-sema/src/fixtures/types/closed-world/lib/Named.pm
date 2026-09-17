package Named;
use strict;
use warnings;

# Globs, and every one of them into a package that is on the page: this one,
# and `Elsewhere`. Neither can be whoever `use`s `Named`, so none of it is a
# hole in an importer's method surface — the shape `Carp` has.
sub setup {
    no strict 'refs';
    *own_helper            = sub { 1 };
    *Named::other_helper   = sub { 2 };
    *{"Elsewhere::helper"} = sub { 3 };
}

1;
