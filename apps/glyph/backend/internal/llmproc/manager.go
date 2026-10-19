// Package llmproc manages locally-spawned LLM server processes
// (typically `llama-server` from llama.cpp).
//
// The manager tracks processes keyed by the configured LLM model name. Each
// process gets its own bounded ring buffer of recent stdout/stderr output so
// the UI can show logs when something fails to boot.
//
// The manager itself does not know anything about llama.cpp's CLI surface —
// callers are responsible for assembling the argv. This keeps the package
// reusable if we ever want to run a different backend.
package llmproc

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"sync"
	"time"
)

// logBufferLines is the per-process ring buffer size for captured output.
const logBufferLines = 200

// Manager owns the lifecycle of managed LLM processes.
type Manager struct {
	mu    sync.Mutex
	procs map[string]*proc
}

// proc is one running child process and its captured output.
type proc struct {
	name     string
	cmd      *exec.Cmd
	started  time.Time
	exited   bool
	exitErr  error
	logMu    sync.Mutex
	logLines []string
}

// Status is the public view of a managed process.
type Status struct {
	Name      string   `json:"name"`
	Managed   bool     `json:"managed"`
	PID       int      `json:"pid,omitempty"`
	StartedAt string   `json:"started_at,omitempty"`
	Exited    bool     `json:"exited,omitempty"`
	ExitError string   `json:"exit_error,omitempty"`
	Recent    []string `json:"recent_output,omitempty"`
}

// ErrNotRunning is returned by Stop when there is no managed process for the
// given name (either never started or already stopped and reaped).
var ErrNotRunning = errors.New("llmproc: no managed process for that name")

// ErrAlreadyRunning is returned by Start when a process with the same name is
// already managed and alive.
var ErrAlreadyRunning = errors.New("llmproc: a managed process with that name is already running")

// New creates a fresh, empty manager.
func New() *Manager {
	return &Manager{procs: make(map[string]*proc)}
}

// Start spawns `binary argv...` and registers it under name. It returns
// immediately; the caller should poll Status to see whether the child stayed
// alive long enough to accept requests.
func (m *Manager) Start(name, binary string, argv []string) (*Status, error) {
	if name == "" {
		return nil, errors.New("llmproc: name is required")
	}
	if binary == "" {
		return nil, errors.New("llmproc: binary is required")
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	if existing, ok := m.procs[name]; ok && !existing.exited {
		return nil, ErrAlreadyRunning
	}

	cmd := exec.Command(binary, argv...)
	// Giving the child its own process group means Stop can signal the whole
	// tree (llama-server sometimes spawns helper threads that survive a plain
	// SIGTERM to the leader).
	cmd.SysProcAttr = processGroupAttr()

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, fmt.Errorf("stdout pipe: %w", err)
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return nil, fmt.Errorf("stderr pipe: %w", err)
	}

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("start %s: %w", binary, err)
	}

	p := &proc{
		name:    name,
		cmd:     cmd,
		started: time.Now(),
	}
	m.procs[name] = p

	go p.capture(stdout)
	go p.capture(stderr)
	go func() {
		err := cmd.Wait()
		p.logMu.Lock()
		p.exited = true
		p.exitErr = err
		p.logMu.Unlock()
	}()

	return p.snapshot(), nil
}

// Stop signals the managed process for name to shut down. It waits up to
// `grace` for a clean exit before escalating to SIGKILL. A missing entry
// returns ErrNotRunning.
func (m *Manager) Stop(name string, grace time.Duration) error {
	m.mu.Lock()
	p, ok := m.procs[name]
	m.mu.Unlock()

	if !ok {
		return ErrNotRunning
	}

	p.logMu.Lock()
	alreadyExited := p.exited
	p.logMu.Unlock()
	if alreadyExited {
		m.forget(name)
		return nil
	}

	if p.cmd.Process != nil {
		_ = terminateProcess(p.cmd.Process.Pid)
	}

	deadline := time.After(grace)
	tick := time.NewTicker(100 * time.Millisecond)
	defer tick.Stop()
	for {
		select {
		case <-deadline:
			if p.cmd.Process != nil {
				_ = killProcess(p.cmd.Process.Pid)
			}
			m.forget(name)
			return nil
		case <-tick.C:
			p.logMu.Lock()
			done := p.exited
			p.logMu.Unlock()
			if done {
				m.forget(name)
				return nil
			}
		}
	}
}

// Status returns a snapshot of the managed process for name, or nil if there
// isn't one.
func (m *Manager) Status(name string) *Status {
	m.mu.Lock()
	p, ok := m.procs[name]
	m.mu.Unlock()
	if !ok {
		return nil
	}
	return p.snapshot()
}

// StopAll tears down every managed process. Intended for server shutdown.
func (m *Manager) StopAll(grace time.Duration) {
	m.mu.Lock()
	names := make([]string, 0, len(m.procs))
	for n := range m.procs {
		names = append(names, n)
	}
	m.mu.Unlock()
	for _, n := range names {
		_ = m.Stop(n, grace)
	}
}

func (m *Manager) forget(name string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.procs, name)
}

func (p *proc) snapshot() *Status {
	p.logMu.Lock()
	defer p.logMu.Unlock()

	s := &Status{
		Name:      p.name,
		Managed:   true,
		StartedAt: p.started.UTC().Format(time.RFC3339),
		Exited:    p.exited,
	}
	if p.cmd != nil && p.cmd.Process != nil {
		s.PID = p.cmd.Process.Pid
	}
	if p.exitErr != nil {
		s.ExitError = p.exitErr.Error()
	}
	if len(p.logLines) > 0 {
		s.Recent = append([]string{}, p.logLines...)
	}
	return s
}

func (p *proc) capture(r io.ReadCloser) {
	defer r.Close()
	sc := bufio.NewScanner(r)
	sc.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for sc.Scan() {
		line := sc.Text()
		p.logMu.Lock()
		p.logLines = append(p.logLines, line)
		if len(p.logLines) > logBufferLines {
			p.logLines = p.logLines[len(p.logLines)-logBufferLines:]
		}
		p.logMu.Unlock()
	}
}
