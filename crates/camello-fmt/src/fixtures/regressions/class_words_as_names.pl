# `class`, `field` and `method` are keywords only in the shapes that make them
# ones — see core_class_feature.pl. Outside a class, and to perl outside
# `use feature 'class'`, they are ordinary names, and a file that never mentions
# the feature is full of them.
sub method { return 1 }

sub field { return 2 }

my %h = (class => 1, field => 2, method => 3);
my $x = Foo->class + method($h{field}) + field();
my $y = $h{method};
