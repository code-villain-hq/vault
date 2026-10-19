# API Documentation

## Base URL

```
http://localhost:3000/api
```

## Response Format

All responses are JSON. Success responses include the requested data, error responses include an `error` field.

## Health & Status

### GET /health

Health check endpoint.

**Response:**
```json
{
  "status": "healthy"
}
```

### GET /languages

Get all available languages with metadata.

**Response:**
```json
[
  {
    "code": "en",
    "translation_count": 42
  },
  {
    "code": "sv",
    "translation_count": 38
  }
]
```

## Configuration

### GET /config

Get current configuration.

**Response:**
```json
{
  "app": {
    "port": 3000
  },
  "user_preferences": {
    "source_language": "en",
    "default_target_language": "sv"
  },
  "language_pairs": {
    "en_sv": {
      "model": "mistral",
      "confidence_threshold": 0.7
    }
  }
}
```

### POST /config

Update configuration.

**Request Body:**
```json
{
  "user_preferences": {
    "source_language": "en",
    "default_target_language": "sv"
  }
}
```

**Response:**
```json
{
  "message": "Configuration updated successfully",
  "config": { ... }
}
```

## Translations

### GET /translations/{lang}

Get all translations for a language.

**Parameters:**
- `lang` (string, required) - Language code (e.g., "sv", "fi")

**Response:**
```json
{
  "greeting": "Hej",
  "farewell": "Hej då",
  "welcome": "Välkommen"
}
```

### GET /translations/{lang}/missing

Get missing translations for a target language.

**Parameters:**
- `lang` (string, required) - Target language code
- `source` (string, optional) - Source language code (default: "en")

**Response:**
```json
[
  {
    "key": "new_feature",
    "source": "New Feature"
  },
  {
    "key": "help_text",
    "source": "Click here for help"
  }
]
```

### GET /translations/{lang}/low-confidence

Get translations with confidence below threshold.

**Parameters:**
- `lang` (string, required) - Language code
- `threshold` (float, optional) - Confidence threshold 0.0-1.0 (default: 0.7)

**Response:**
```json
[
  {
    "key": "greeting",
    "translation": "Hejsan",
    "confidence": 0.65,
    "translated_by": "mistral",
    "timestamp": "2024-04-18T12:00:00Z"
  }
]
```

## Translation Operations

### POST /translate/single

Translate a single key.

**Request Body:**
```json
{
  "key": "greeting",
  "target_lang": "sv",
  "source_lang": "en"
}
```

**Response:**
```json
{
  "key": "greeting",
  "translation": "Hej",
  "confidence": 0.95
}
```

### POST /translate/missing

Translate all missing keys for one or more target languages.

**Request Body:**
```json
{
  "target_langs": ["sv", "fi"],
  "source_lang": "en"
}
```

**Response:**
```json
{
  "message": "Translation completed",
  "results": {
    "sv": {
      "translated": 5,
      "low_confidence": 1
    },
    "fi": {
      "translated": 5,
      "low_confidence": 0
    }
  }
}
```

## Metadata

### GET /metadata/{lang}

Get metadata for a language (confidence scores, timestamps, etc.).

**Parameters:**
- `lang` (string, required) - Language code

**Response:**
```json
{
  "language": "sv",
  "metadata": {
    "greeting": {
      "confidence": 0.95,
      "translated_by": "mistral",
      "timestamp": "2024-04-18T12:00:00Z"
    }
  }
}
```

### POST /metadata/{lang}/{key}

Update metadata for a specific translation.

**Parameters:**
- `lang` (string, required) - Language code
- `key` (string, required) - Translation key

**Request Body:**
```json
{
  "confidence": 0.9,
  "translated_by": "manual",
  "timestamp": "2024-04-18T12:30:00Z"
}
```

**Response:**
```json
{
  "message": "Metadata updated successfully"
}
```

## Error Responses

All error responses follow this format:

```json
{
  "error": "Error description"
}
```

### Common HTTP Status Codes

- `200 OK` - Successful request
- `400 Bad Request` - Invalid parameters
- `404 Not Found` - Resource not found
- `500 Internal Server Error` - Server error

### Example Error Response

```json
{
  "error": "Language 'xx' not found"
}
```

## Rate Limiting

Currently no rate limiting is implemented. Large bulk translation requests may take time depending on the LLM model and system resources.

## Authentication

No authentication is currently required. The tool is designed for local use only.

## Pagination

Pagination is not currently implemented. All results are returned in full.

## Examples

### cURL

```bash
# Get health status
curl http://localhost:3000/api/health

# Get available languages
curl http://localhost:3000/api/languages

# Get Swedish translations
curl http://localhost:3000/api/translations/sv

# Get missing Swedish translations
curl "http://localhost:3000/api/translations/sv/missing?source=en"

# Translate missing Swedish keys
curl -X POST http://localhost:3000/api/translate/missing \
  -H "Content-Type: application/json" \
  -d '{
    "target_langs": ["sv"],
    "source_lang": "en"
  }'
```

### JavaScript/Fetch

```javascript
// Get configuration
const config = await fetch('/api/config').then(r => r.json());

// Translate missing keys
const result = await fetch('/api/translate/missing', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    target_langs: ['sv', 'fi'],
    source_lang: 'en'
  })
}).then(r => r.json());
```

## Implementation Notes

### Confidence Scores

Confidence scores range from 0.0 to 1.0:
- `0.9+` - Very high confidence (excellent translation)
- `0.7-0.9` - Good confidence (reliable translation)
- `0.5-0.7` - Medium confidence (review recommended)
- `<0.5` - Low confidence (manual review required)

The LLM model determines the confidence score based on its internal assessment of the translation quality.

### Translation File Format

Translation files must be valid JSON with key-value pairs:

```json
{
  "key1": "value1",
  "key2": "value2"
}
```

Nested keys can be supported with dot notation (e.g., `"menu.file.open"`).

### Metadata Storage

Metadata is stored separately from translations to keep translation files clean and prevent conflicts with existing systems.

Each translation can have:
- `confidence`: Float 0.0-1.0 indicating translation quality
- `translated_by`: Name of the LLM model that provided the translation
- `timestamp`: ISO 8601 timestamp of when the translation was created/updated
