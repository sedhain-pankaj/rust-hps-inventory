
examples = [
    'enroll',
    'identify',
    'img-capture',
    'manage-prints',
    'verify',
    'clear-storage',
]

foreach example: examples
    executable(example,
        [ example + '.c', 'storage.c', 'utilities.c' ],
        dependencies: [
            libfprint_dep,
            glib_dep,
        ],
    )
endforeach

executable('employee-clock-helper',
    'employee-clock-helper.c',
    dependencies: [
        libfprint_private_dep,
        glib_dep,
    ],
)

executable('fp-capture',
    'fp-capture.c',
    dependencies: [
        libfprint_dep,
        glib_dep,
    ],
)

executable('cpp-test',
    'cpp-test.cpp',
    dependencies: libfprint_dep,
)

executable('fp-align-tool',
    'fp-align-tool.c',
    dependencies: [
        libfprint_private_dep,
        dependency('opencv4'),
    ],
)

if installed_tests
    install_subdir('prints',
        install_dir: installed_tests_testdir)
endif
