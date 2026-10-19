//go:build windows

package llmproc

import (
	"os"
	"syscall"
)

// processGroupAttr is a no-op on Windows; there's no Setpgid equivalent we
// rely on. Managed processes still live in the same console, which is fine.
func processGroupAttr() *syscall.SysProcAttr {
	return &syscall.SysProcAttr{}
}

// terminateProcess falls back to a plain process kill on Windows; there's no
// POSIX-style signal delivery so we can't differentiate SIGTERM vs SIGKILL.
func terminateProcess(pid int) error {
	p, err := os.FindProcess(pid)
	if err != nil {
		return err
	}
	return p.Kill()
}

func killProcess(pid int) error {
	return terminateProcess(pid)
}
