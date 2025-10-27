# Array element followed by hash key - no space
$_[0]{rbuf};

# Multiple levels of array/hash access
$_[0][1]{key};
$array[0]{hash}[1];

# Chained hash access
$hash{a}{b}{c};

# Mixed complex cases
$obj->{array}[0]{key};
$data[0]{nested}[1]{deep};
