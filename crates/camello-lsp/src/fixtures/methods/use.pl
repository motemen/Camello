use strict;
use warnings;
use Dog;

my $dog = Dog->new(name => 'Rex');
#   ^ hover $dog : InstanceOf['Dog']
print $dog->speak;
#           ^ hover Dog::speak($self : Any) -> Str
#           ^ definition Dog.pm:7:5
#          ^ complete-own speak, legs, new, name
print $dog->name;
#           ^ definition Animal.pm:11:5
