ALTER TABLE forecast_files
ADD COLUMN parsed_forecast JSON;
ALTER TABLE forecast_files
ADD COLUMN schema_version TEXT;
