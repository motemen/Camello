try {
    do_something();
} catch ($error) {
    warn $error;
} finally {
    cleanup();
}

try {
    maybe();
} catch {
    fallback();
};

my $value = try { compute() } catch { recover() };
