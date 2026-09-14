package run

import (
	"os"
	"strconv"
	"strings"
	"testing"
)

// helperEnv switches the test binary into a stand-in for a task process, so
// exit-code propagation can be tested without depending on npm or a shell.
const helperEnv = "PKGR_TEST_HELPER_EXIT"

func TestMain(m *testing.M) {
	if code, ok := os.LookupEnv(helperEnv); ok {
		n, err := strconv.Atoi(code)
		if err != nil {
			n = 99
		}
		os.Exit(n)
	}
	os.Exit(m.Run())
}

// runHelper invokes this test binary as the child process.
func runHelper(t *testing.T, dir string, exitCode int) (int, error) {
	t.Helper()
	t.Setenv(helperEnv, strconv.Itoa(exitCode))
	return Run(dir, os.Args[0], nil)
}

func TestRunPropagatesExitCode(t *testing.T) {
	for _, want := range []int{0, 1, 3, 42} {
		t.Run(strconv.Itoa(want), func(t *testing.T) {
			got, err := runHelper(t, t.TempDir(), want)
			if err != nil {
				t.Fatalf("Run returned an error: %v", err)
			}
			if got != want {
				t.Errorf("exit code = %d, want %d", got, want)
			}
		})
	}
}

// A non-zero exit is the task failing, not pkgr failing, so it must not come
// back as a Go error.
func TestRunFailureIsNotAnError(t *testing.T) {
	code, err := runHelper(t, t.TempDir(), 7)
	if err != nil {
		t.Fatalf("err = %v, want nil", err)
	}
	if code != 7 {
		t.Errorf("exit code = %d, want 7", code)
	}
}

func TestRunMissingBinary(t *testing.T) {
	_, err := Run(t.TempDir(), "pkgr-definitely-not-a-real-binary", []string{"run", "dev"})
	if err == nil {
		t.Fatal("expected an error")
	}
	if !strings.Contains(err.Error(), "not found on your PATH") {
		t.Errorf("err = %v, want a PATH error", err)
	}
}
