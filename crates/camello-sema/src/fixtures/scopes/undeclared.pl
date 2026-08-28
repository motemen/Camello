use strict;
use warnings;

my $known = 1;
print $known;
print $unknown;                 #~ error undeclared-variable: `$unknown`
print "interpolated $missing";  #~ error undeclared-variable: `$missing`

my %options = (a => 1);
print $options{a};
print $other{a};                #~ error undeclared-variable: `%other`

my @items = (1, 2);
print $items[0];
print $rows[0];                 #~ error undeclared-variable: `@rows`

# A package variable named in full is never a lexical, so it is never this
# diagnostic's business.
print $Foo::Bar::setting;

# perl binds these itself.
print "$_ $0 $ENV{HOME} $1";
for (@items) { print }
