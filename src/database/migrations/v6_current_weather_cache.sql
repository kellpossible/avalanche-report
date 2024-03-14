CREATE TABLE IF NOT EXISTS current_weather_cache (
    weather_station_id TEXT NOT NULL PRIMARY KEY,
    data JSON NOT NULL
);
