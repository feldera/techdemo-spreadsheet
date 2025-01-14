#!/bin/bash
set -ex

FILE_PATH="/tmp/spreadsheet_data_restore.parquet"
BUCKET_NAME="spreadsheet-backups"

# Get the most recent file from the S3 bucket
echo "Fetching the most recent backup file from s3://$BUCKET_NAME"
LATEST_FILE=$(aws s3 ls "s3://$BUCKET_NAME/" | sort | tail -1 | awk '{print $4}')

# Check if a file was found
if [[ -z "$LATEST_FILE" ]]; then
  echo "Error: No files found in s3://$BUCKET_NAME/"
  exit 1
fi

echo "Most recent backup file: $LATEST_FILE"

# Download the file
aws s3 cp "s3://$BUCKET_NAME/$LATEST_FILE" "$FILE_PATH"

# Verify the download
if [[ $? -ne 0 || ! -f "$FILE_PATH" ]]; then
  echo "Error: Failed to download the file from s3://$BUCKET_NAME/$LATEST_FILE"
  exit 1
fi

echo "File downloaded successfully to $FILE_PATH"

fda restart xls
sleep 5

# Insert the data back into Feldera
echo "Inserting data back into Feldera"
curl -X POST \
  --data-binary @$FILE_PATH \
  "${FELDERA_HOST}/v0/pipelines/xls/ingress/spreadsheet_data?format=parquet" \
  -H "Authorization: Bearer ${FELDERA_API_KEY}"

# Verify the insertion
if [[ $? -eq 0 ]]; then
  echo "Data successfully inserted back into Feldera."
else
  echo "Error: Data insertion failed."
  exit 1
fi
