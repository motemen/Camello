# The base class, as a stub: `Class::Accessor` is what a subclass inherits
# `new` and the `mk_*` family from, and without it every subclass has an
# ancestor the run never saw — which makes "no such method" unsayable.
package Class::Accessor;

sub new ($class, $fields = undef) {}
sub mk_accessors ($class, @fields) {}
sub mk_ro_accessors ($class, @fields) {}
sub mk_wo_accessors ($class, @fields) {}
sub follow_best_practice ($class) {}

1;
