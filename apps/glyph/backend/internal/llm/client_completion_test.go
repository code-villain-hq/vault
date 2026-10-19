package llm

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestExtractGeneratedText_nativeContent(t *testing.T) {
	s, err := extractGeneratedText(map[string]interface{}{"content": `{"translation":"x","confidence":1}`})
	if err != nil || s != `{"translation":"x","confidence":1}` {
		t.Fatalf("got %q, err %v", s, err)
	}
}

func TestExtractGeneratedText_openAIChoicesText(t *testing.T) {
	body := map[string]interface{}{
		"choices": []interface{}{
			map[string]interface{}{"text": `{"translation":"Hallo","confidence":0.9}`},
		},
	}
	s, err := extractGeneratedText(body)
	if err != nil || s != `{"translation":"Hallo","confidence":0.9}` {
		t.Fatalf("got %q, err %v", s, err)
	}
}

func TestParseResponse_LastTranslationObjectWins(t *testing.T) {
	c := NewClient("http://127.0.0.1:1", "m", false)
	garbage := `{"translation": "wrong", "confidence": 0.1}
Text to translate: "hello"
JSON Response:
{"translation": "{{count}} ok", "confidence": 0.95}
more noise {"translation": "final", "confidence": 0.88}`
	tr, conf := c.parseResponse(garbage)
	if tr != "final" || conf != 0.88 {
		t.Fatalf("got translation %q conf %v", tr, conf)
	}
}

func TestNewClient_LocalhostRewrittenToIPv4Loopback(t *testing.T) {
	c := NewClient("http://localhost:8081/", "m", false)
	if got := c.Endpoint(); got != "http://127.0.0.1:8081" {
		t.Fatalf("endpoint %q, want http://127.0.0.1:8081", got)
	}
}

func TestTranslate_OpenAIChoicesResponse(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/completion" {
			t.Errorf("path %s", r.URL.Path)
		}
		_ = json.NewEncoder(w).Encode(map[string]interface{}{
			"object": "text_completion",
			"choices": []interface{}{
				map[string]interface{}{"text": `{"translation":"Ja","confidence":0.95}`},
			},
		})
	}))
	defer srv.Close()

	c := NewClient(srv.URL, "EuroLLM", false)
	resp, err := c.Translate("en", "de", "Yes")
	if err != nil {
		t.Fatal(err)
	}
	if resp.Text != "Ja" {
		t.Errorf("translation %q", resp.Text)
	}
}
