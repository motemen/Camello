# Variable declarations
my $x = 1;
our $x=2;
state $x=3;
local $x=4;
my@arr=(1,2,3);
our%hash=(a=>1);
state($x,$y)=(1,2);
my*glob=\*STDIN;

# Local lvalue variations
local $SIG{__WARN__} = \&CORE::die;
local $ENV{PATH} = '/usr/bin';
local $hash{key} = $value;
local Module->hash->{key} = $value;
local M->lvar = 1;
local $array[0] = 'first';
local $list[1] = $item;
local ($a, $b) = (1, 2);
local ($x, $y, $z) = @values;
local ($SIG{__WARN__}, $a) = (\&handler, $old_a);
local ($array[0], $hash{key}) = ($new_first, $new_value);
local (undef, $SIG{__DIE__}) = (undef, \&my_die);
local ($a, undef, $hash{key}) = @list;

# Undef in declarations and calls
my(undef,$x)=@_;
my($a,undef,$c)=@list;
my(undef,undef,$result)=func();
our(undef,$y)=(1,2);
state($x,undef)=@array;
(undef,my @a)=@_;
(my $x,undef,our @y)=get_values();
(undef,state $cache,my %hash)=complex_func(@args);
(local $old,undef)=backup();

undef $x;
undef($var);
undef @array;
undef %hash;
undef $hash{key};
undef $array[0];
undef$x;
undef	$y;
undef $x,$y;
undef($a,$b,$c);
my $x = undef;
$y = undef;
return undef;
undef $x; my $y = undef;

# Phase blocks
BEGIN{say"hi";}UNITCHECK{say"unit";}INIT{my $x=1;}CHECK{warn 1;}END{say 'bye';}
sub foo { BEGIN { warn "hi"; } }
